use std::collections::HashMap;

use mmb_utils::DateTime;
use rust_decimal_macros::dec;

use crate::{
    exchanges::common::{Amount, MarketId, Price},
    misc::price_by_order_side::PriceByOrderSide,
    order_book::local_snapshot_service::LocalSnapshotsService,
    services::usd_convertion::{
        price_source_chain::PriceSourceChain, rebase_price_step::RebaseDirection,
    },
};

#[cfg(test)]
pub(crate) fn calculate(
    src_amount: Amount,
    price_source_chain: &PriceSourceChain,
    prices: &HashMap<MarketId, Price>,
) -> Price {
    calculate_amount_for_chain(src_amount, price_source_chain, |market_id| {
        prices.get(&market_id).cloned()
    })
    .expect("Invalid price cache")
}

fn calculate_amount_for_chain(
    src_amount: Amount,
    price_source_chain: &PriceSourceChain,
    calculate_price: impl Fn(MarketId) -> Option<Price>,
) -> Option<Amount> {
    let mut rebase_price = dec!(1);

    for step in &price_source_chain.rebase_price_steps {
        let market_id = MarketId::new(step.exchange_id, step.symbol.currency_pair());
        let calculated_price = (calculate_price)(market_id)?;

        match step.direction {
            RebaseDirection::ToQuote => rebase_price *= calculated_price,
            RebaseDirection::ToBase => rebase_price /= calculated_price,
        }
    }
    Some(rebase_price * src_amount)
}

pub(crate) fn convert_amount(
    src_amount: Amount,
    local_snapshot_service: &LocalSnapshotsService,
    price_source_chain: &PriceSourceChain,
) -> Option<Amount> {
    calculate_amount_for_chain(src_amount, price_source_chain, |market_id| {
        local_snapshot_service
            .get_snapshot(market_id)?
            .calculate_middle_price(market_id)
    })
}

pub fn convert_amount_in_past(
    src_amount: Amount,
    price_cache: &HashMap<MarketId, PriceByOrderSide>,
    time_in_past: DateTime,
    price_source_chain: &PriceSourceChain,
) -> Option<Amount> {
    calculate_amount_for_chain(src_amount, price_source_chain, |market_id| {
        let prices = match price_cache.get(&market_id) {
            Some(prices) => prices,
            None => {
                log::error!("Can't get price {:?} on time {}", market_id, time_in_past);
                return None;
            }
        };

        let top_bid = prices.top_ask?;
        let top_ask = prices.top_bid?;

        Some((top_ask + top_bid) * dec!(0.5))
    })
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use chrono::Utc;
    use mmb_utils::hashmap;
    use mockall_double::double;
    use parking_lot::ReentrantMutexGuard;

    #[double]
    use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;

    use crate::order_book_data;
    use crate::{
        exchanges::{
            common::CurrencyPair, general::test_helper::get_test_exchange_by_currency_codes,
        },
        services::usd_convertion::{
            price_source_chain::PriceSourceChain,
            price_source_service::{test::PriceSourceServiceTestBase, PriceSourceService},
        },
        settings::{CurrencyPriceSourceSettings, ExchangeIdCurrencyPairSettings},
    };

    use super::*;

    fn generate_one_step_setup() -> (
        CurrencyPair,
        PriceSourceChain,
        ReentrantMutexGuard<'static, ()>,
    ) {
        let base_currency_code = "USDT".into();
        let quote_currency_code = "BTC".into();
        let currency_pair = CurrencyPair::from_codes(base_currency_code, quote_currency_code);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            quote_currency_code,
            base_currency_code,
            vec![ExchangeIdCurrencyPairSettings {
                exchange_account_id: PriceSourceServiceTestBase::exchange_account_id(),
                currency_pair,
            }],
        )];

        let (mut converter, locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_by_currency_codes(false, "USDT", "BTC")
                } else {
                    panic!(
                        "Unknown exchange in CurrencyPairToSymbolConverter:{:?}",
                        exchange_account_id
                    )
                }
                .0
                .get_symbol(currency_pair)
                .expect("failed to get currency pair")
            });

        let price_source_chains = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );

        let price_source_chain = price_source_chains
            .into_iter()
            .find(|chain| {
                chain.start_currency_code == quote_currency_code
                    && chain.end_currency_code == base_currency_code
            })
            .expect("in test");

        (currency_pair, price_source_chain, locker)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_amount_now_using_one_step_with_price() {
        let (currency_pair, price_source_chain, _locker) = generate_one_step_setup();

        let snapshot = order_book_data![
            dec!(10) => dec!(1.2),
            dec!(12) => dec!(4.3),
            ;
            dec!(1) => dec!(6),
            dec!(2) => dec!(9),
        ]
        .to_local_order_book_snapshot();

        let market_id = MarketId::new(PriceSourceServiceTestBase::exchange_id(), currency_pair);

        let snapshot_service = LocalSnapshotsService::new(hashmap![market_id => snapshot]);

        let src_amount = dec!(10);
        let price_now =
            convert_amount(src_amount, &snapshot_service, &price_source_chain).expect("in test");

        assert_eq!(dec!(1) / (dec!(12) / dec!(2)) * src_amount, price_now);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_amount_now_using_one_step_without_price() {
        let (currency_pair, price_source_chain, _locker) = generate_one_step_setup();
        let snapshot = order_book_data!().to_local_order_book_snapshot();

        let market_id = MarketId::new(PriceSourceServiceTestBase::exchange_id(), currency_pair);

        let snapshot_service = LocalSnapshotsService::new(hashmap![market_id => snapshot]);

        let src_amount = dec!(10);
        let price_now = convert_amount(src_amount, &snapshot_service, &price_source_chain);

        assert!(price_now.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_amount_in_past_using_one_step_with_price() {
        let (currency_pair, price_source_chain, _locker) = generate_one_step_setup();
        let time_in_past = Utc::now();
        let market_id = MarketId::new(PriceSourceServiceTestBase::exchange_id(), currency_pair);
        let price_cache = hashmap![
            market_id => PriceByOrderSide::new(Some(dec!(10)), Some(dec!(2)))
        ];

        let src_amount = dec!(10);
        let price_now =
            convert_amount_in_past(src_amount, &price_cache, time_in_past, &price_source_chain)
                .expect("in test");

        assert_eq!(dec!(1) / (dec!(12) / dec!(2)) * src_amount, price_now);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_amount_in_past_using_one_step_without_price() {
        let (_, price_source_chain, _locker) = generate_one_step_setup();
        let time_in_past = Utc::now();
        let price_cache = HashMap::new();
        let src_amount = dec!(10);
        let price_now =
            convert_amount_in_past(src_amount, &price_cache, time_in_past, &price_source_chain);

        assert!(price_now.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_amount_with_current_cached_prices_using_one_step_with_price() {
        let (currency_pair, price_source_chain, _locker) = generate_one_step_setup();
        let cached_price = dec!(6);
        let market_id = MarketId::new(PriceSourceServiceTestBase::exchange_id(), currency_pair);
        let price_cache = hashmap![market_id => cached_price];

        let src_amount = dec!(10);
        let price_now = calculate(src_amount, &price_source_chain, &price_cache);

        assert_eq!(dec!(1) / cached_price * src_amount, price_now);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "Invalid price cache")]
    async fn calculate_amount_with_current_cached_prices_using_one_step_without_price() {
        let (_, price_source_chain, _locker) = generate_one_step_setup();
        let price_cache = HashMap::new();

        let src_amount = dec!(10);
        let _ = calculate(src_amount, &price_source_chain, &price_cache);
    }

    struct TwoStepSetup {
        currency_pair_1: CurrencyPair,
        currency_pair_2: CurrencyPair,
        price_source_chain: PriceSourceChain,
    }

    fn generate_two_step_setup() -> (TwoStepSetup, ReentrantMutexGuard<'static, ()>) {
        let base_currency_code_1 = "USDT".into();
        let quote_currency_code_1 = "BTC".into();
        let currency_pair_1 = CurrencyPair::from_codes(base_currency_code_1, quote_currency_code_1);

        let base_currency_code_2 = "BTC".into();
        let quote_currency_code_2 = "EOS".into();
        let currency_pair_2 = CurrencyPair::from_codes(base_currency_code_2, quote_currency_code_2);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            quote_currency_code_2,
            base_currency_code_1,
            vec![
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id(),
                    currency_pair: currency_pair_1,
                },
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id_2(),
                    currency_pair: currency_pair_2,
                },
            ],
        )];

        let (mut converter, locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_by_currency_codes(false, "USDT", "BTC")
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_2()
                {
                    get_test_exchange_by_currency_codes(false, "BTC", "EOS")
                } else {
                    panic!(
                        "Unknown exchange in CurrencyPairToSymbolConverter:{:?}",
                        exchange_account_id
                    )
                }
                .0
                .get_symbol(currency_pair)
                .expect("failed to get currency pair")
            });

        let price_source_chains = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );
        let price_source_chain = price_source_chains
            .into_iter()
            .find(|chain| {
                chain.start_currency_code == quote_currency_code_2
                    && chain.end_currency_code == base_currency_code_1
            })
            .expect("in test");

        (
            TwoStepSetup {
                currency_pair_1,
                currency_pair_2,
                price_source_chain,
            },
            locker,
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn calculate_amount_with_current_cached_prices_using_two_step_with_price() {
        let (setup, _locker) = generate_two_step_setup();
        let market_id_1 = MarketId::new(
            PriceSourceServiceTestBase::exchange_id(),
            setup.currency_pair_1,
        );
        let market_id_2 = MarketId::new(
            PriceSourceServiceTestBase::exchange_id(),
            setup.currency_pair_2,
        );
        let cached_price_1 = dec!(6);
        let cached_price_2 = dec!(7);
        let price_cache = hashmap![
            market_id_1 => cached_price_1,
            market_id_2 => cached_price_2
        ];

        let src_amount = dec!(10);
        let price_now = calculate(src_amount, &setup.price_source_chain, &price_cache);

        assert_eq!(
            dec!(1) / cached_price_1 / cached_price_2 * src_amount,
            price_now
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "Invalid price cache")]
    async fn calculate_amount_with_current_cached_prices_using_two_step_without_one_price() {
        let (setup, _locker) = generate_two_step_setup();
        let market_id = MarketId::new(
            PriceSourceServiceTestBase::exchange_id(),
            setup.currency_pair_1,
        );
        let cached_price = dec!(6);
        let price_cache = hashmap![market_id => cached_price];

        let src_amount = dec!(10);
        let _ = calculate(src_amount, &setup.price_source_chain, &price_cache);
    }
}
