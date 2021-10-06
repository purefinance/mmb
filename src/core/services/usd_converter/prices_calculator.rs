use std::collections::HashMap;

use rust_decimal_macros::dec;

use crate::core::{
    exchanges::common::{Amount, Price, TradePlace},
    misc::price_by_order_side::PriceByOrderSide,
    order_book::local_snapshot_service::LocalSnapshotsService,
    services::usd_converter::{
        price_source_chain::PriceSourceChain, rebase_price_step::RebaseDirection,
    },
    DateTime,
};

pub(crate) fn calculate(
    src_amount: Amount,
    prices_source_chain: &PriceSourceChain,
    prices: &HashMap<TradePlace, Price>,
) -> Price {
    calculate_amount_for_chain(src_amount, prices_source_chain, |trade_place| {
        prices.get(trade_place).cloned()
    })
    .expect("Invalid price cache")
}

fn calculate_amount_for_chain(
    src_amount: Amount,
    prices_source_chain: &PriceSourceChain,
    calculate_price: impl Fn(&TradePlace) -> Option<Price>,
) -> Option<Amount> {
    let mut rebase_price = dec!(1);

    for step in &prices_source_chain.rebase_price_steps {
        let trade_place = TradePlace::new(
            step.exchange_id.clone(),
            step.currency_pair_metadata.currency_pair(),
        );
        let calculated_price = (calculate_price)(&trade_place)?;

        match step.direction {
            RebaseDirection::ToQuote => rebase_price *= calculated_price,
            RebaseDirection::ToBase => rebase_price /= calculated_price,
        }
    }
    Some(rebase_price * src_amount)
}

pub(crate) fn convert_amount_now(
    src_amount: Amount,
    local_snapshot_service: &LocalSnapshotsService,
    prices_source_chain: &PriceSourceChain,
) -> Option<Amount> {
    calculate_amount_for_chain(src_amount, prices_source_chain, |trade_place| {
        local_snapshot_service
            .get_snapshot(trade_place)?
            .calculate_middle_price(trade_place)
    })
}

pub fn convert_amount_in_past(
    src_amount: Amount,
    price_cache: HashMap<TradePlace, PriceByOrderSide>,
    time_in_past: DateTime,
    prices_source_chain: &PriceSourceChain,
) -> Option<Amount> {
    calculate_amount_for_chain(src_amount, prices_source_chain, |trade_place| {
        let prices = match price_cache.get(trade_place) {
            Some(prices) => prices,
            None => {
                log::error!("Can't get price {:?} on time {}", trade_place, time_in_past);
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

    use crate::{
        core::{
            exchanges::{
                common::{CurrencyCode, CurrencyPair},
                general::{
                    currency_pair_metadata::{CurrencyPairMetadata, Precision},
                    currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter,
                    test_helper::get_test_exchange_with_currency_pair_metadata,
                },
            },
            services::usd_converter::{
                price_source_chain::PriceSourceChain,
                price_source_service::{test::PriceSourceServiceTestBase, PriceSourceService},
            },
            settings::{CurrencyPriceSourceSettings, ExchangeIdCurrencyPairSettings},
        },
        hashmap,
    };

    use super::*;

    fn getenerate_one_step_setup() -> (CurrencyPair, PriceSourceChain) {
        let base_currency_code = CurrencyCode::new("USDT".into());
        let quote_currency_code = CurrencyCode::new("BTC".into());
        let currency_pair = CurrencyPair::from_codes(&base_currency_code, &quote_currency_code);

        let currency_pair_metadata = Arc::new(CurrencyPairMetadata::new(
            false,
            false,
            base_currency_code.as_str().into(),
            base_currency_code.clone(),
            quote_currency_code.as_str().into(),
            quote_currency_code.clone(),
            None,
            None,
            None,
            None,
            None,
            base_currency_code.clone(),
            None,
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(0) },
        ));

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            quote_currency_code.clone(),
            base_currency_code.clone(),
            vec![ExchangeIdCurrencyPairSettings {
                exchange_account_id: PriceSourceServiceTestBase::get_exchange_account_id(),
                currency_pair: currency_pair.clone(),
            }],
        )];

        let price_source_chains = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(CurrencyPairToMetadataConverter::new(hashmap![
                        PriceSourceServiceTestBase::get_exchange_account_id() => get_test_exchange_with_currency_pair_metadata(
                            currency_pair_metadata.clone()
                        ).0
            ])),
        );

        let price_source_chain = price_source_chains
            .into_iter()
            .find(|chain| {
                chain.start_currency_code == quote_currency_code
                    && chain.end_currency_code == base_currency_code
            })
            .expect("in test");

        (currency_pair, price_source_chain)
    }

    #[test]
    fn calculate_amount_now_using_one_step_with_price() {}

    // [Test]
    //     public void Calculate_AmountNow_UsingOneStep_WithPrice()
    //     {
    //         GenerateOneStepSetup(out var currencyCodePair, out var priceSourceChain);
    //         var asks = new Dictionary<decimal, decimal>
    //         {
    //             {10m, 1.2m},
    //             {12m, 4.3m},
    //         };
    //         var bids = new Dictionary<decimal, decimal>
    //         {
    //             {1m, 6m},
    //             {2m, 9m},
    //         };
    //         var snapshot = new L2OrderBookData(asks, bids).ToLocalOrderBookSnapshot();
    //         var ens = new ExchangeNameSymbol(ExchangeName, currencyCodePair);
    //         var snapshotService = Mock<ILocalSnapshotService>();
    //         snapshotService.Setup(x => x.TryGetSnapshot(ens, out snapshot)).Returns(true);

    //         // act
    //         const decimal sourceAmount = 10m;
    //         var priceSourceCalculator = new PricesCalculator();
    //         var priceNow = priceSourceCalculator.ConvertAmountNow(sourceAmount, snapshotService.Object, priceSourceChain);

    //         // assert
    //         priceNow.Should().BeApproximately(1 / ((10 + 2) / 2m) * sourceAmount, 1e-6m);
    //     }
}
