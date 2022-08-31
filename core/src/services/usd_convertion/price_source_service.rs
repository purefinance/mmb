use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

#[double]
use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;

use crate::{
    infrastructure::spawn_future,
    order_book::local_snapshot_service::LocalSnapshotsService,
    services::usd_convertion::{prices_calculator, rebase_price_step::RebaseDirection},
    settings::CurrencyPriceSourceSettings,
};

use anyhow::{bail, Context, Result};
use domain::events::ExchangeEvent;
use domain::exchanges::symbol::Symbol;
use domain::market::{CurrencyCode, ExchangeId, MarketId};
use domain::order::snapshot::Amount;
use domain::order::snapshot::PriceByOrderSide;
use itertools::Itertools;
use mmb_utils::infrastructure::{SpawnFutureFlags, WithExpect};
use mmb_utils::{cancellation_token::CancellationToken, send_expected::SendExpected, DateTime};
use mockall_double::double;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{
    convert_currency_direction::ConvertCurrencyDirection, price_source_chain::PriceSourceChain,
    price_sources_loader::PriceSourcesLoader, prices_sources_saver::PriceSourcesSaver,
    rebase_price_step::RebasePriceStep,
};

pub struct PriceSourceEventLoop {
    price_sources_saver: PriceSourcesSaver,
    all_market_ids: HashSet<MarketId>,
    local_snapshot_service: LocalSnapshotsService,
    price_cache: HashMap<MarketId, PriceByOrderSide>,
    rx_core: broadcast::Receiver<ExchangeEvent>,
    convert_currency_notification_receiver: mpsc::Receiver<ConvertAmount>,
}

impl PriceSourceEventLoop {
    pub async fn run(
        price_source_chains: Vec<PriceSourceChain>,
        price_sources_saver: PriceSourcesSaver,
        rx_core: broadcast::Receiver<ExchangeEvent>,
        convert_currency_notification_receiver: mpsc::Receiver<ConvertAmount>,
        cancellation_token: CancellationToken,
    ) {
        let run_action = async move {
            let mut this = Self {
                price_sources_saver,
                all_market_ids: Self::map_to_used_market_ids(price_source_chains),
                local_snapshot_service: LocalSnapshotsService::default(),
                price_cache: HashMap::new(),
                rx_core,
                convert_currency_notification_receiver,
            };
            this.run_loop(cancellation_token).await
        };
        spawn_future(
            "PriceSourceService",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            run_action,
        )
        .await
        .expect("Failed to spawn PriceSourceService::run_loop() future");
    }

    async fn run_loop(&mut self, cancellation_token: CancellationToken) -> Result<()> {
        loop {
            tokio::select! {
                main_event_res = self.convert_currency_notification_receiver.recv() => {
                   let convert_amount = main_event_res.context("Error during receiving event on convert_currency_notification_receiver")?;

                    let result = prices_calculator::convert_amount(
                        convert_amount.src_amount,
                        &self.local_snapshot_service,
                        &convert_amount.chain,
                    );
                    convert_amount.task_finished_sender.send_expected(result);
                },
                core_event_res = self.rx_core.recv() => {
                    let event = core_event_res.context("Error during receiving event on rx_core")?;
                    match event {
                        ExchangeEvent::OrderBookEvent(order_book_event) => {
                            let market_id = MarketId::new(
                                order_book_event.exchange_account_id.exchange_id,
                                order_book_event.currency_pair,
                            );
                            if self.all_market_ids.contains(&market_id) {
                                let _ = self.local_snapshot_service.update(order_book_event);
                                self.update_cache_and_save(market_id);
                            }
                        },
                        _ => continue,
                    }
                }
                _ = cancellation_token.when_cancelled() => bail!("main_loop has been stopped by CancellationToken"),
            };
        }
    }

    fn try_update_cache(&mut self, market_id: MarketId, new_value: PriceByOrderSide) -> bool {
        if let Some(old_value) = self.price_cache.get_mut(&market_id) {
            return match old_value == &new_value {
                true => false,
                false => {
                    *old_value = new_value;
                    true
                }
            };
        };

        self.price_cache.insert(market_id, new_value);
        true
    }

    fn update_cache_and_save(&mut self, market_id: MarketId) {
        let snapshot = self.local_snapshot_service.get_snapshot_expected(market_id);

        let price_by_order_side = snapshot.get_top_prices();
        if self.try_update_cache(market_id, price_by_order_side.clone()) {
            self.price_sources_saver
                .save(market_id, price_by_order_side);
        }
    }

    fn map_to_used_market_ids(price_source_chains: Vec<PriceSourceChain>) -> HashSet<MarketId> {
        price_source_chains
            .into_iter()
            .flat_map(|price_source_chain| {
                price_source_chain
                    .rebase_price_steps
                    .into_iter()
                    .map(|step| MarketId::new(step.exchange_id, step.symbol.currency_pair()))
            })
            .collect()
    }
}

pub struct PriceSourceService {
    price_sources_loader: PriceSourcesLoader,
    tx_main: mpsc::Sender<ConvertAmount>,
    convert_currency_notification_receiver: Mutex<Option<mpsc::Receiver<ConvertAmount>>>,
    price_source_chains: HashMap<ConvertCurrencyDirection, PriceSourceChain>,
}

impl PriceSourceService {
    pub fn new(
        currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>,
        price_source_settings: &[CurrencyPriceSourceSettings],
        price_sources_loader: PriceSourcesLoader,
    ) -> Arc<Self> {
        let price_source_chains = Self::prepare_price_source_chains(
            price_source_settings,
            currency_pair_to_symbol_converter,
        );
        let (tx_main, convert_currency_notification_receiver) = mpsc::channel(20_000);

        Arc::new(Self {
            price_sources_loader,
            tx_main,
            convert_currency_notification_receiver: Mutex::new(Some(
                convert_currency_notification_receiver,
            )),
            price_source_chains: price_source_chains
                .into_iter()
                .map(|x| {
                    (
                        ConvertCurrencyDirection::new(x.start_currency_code, x.end_currency_code),
                        x,
                    )
                })
                .collect(),
        })
    }
    pub async fn start(
        self: Arc<Self>,
        price_sources_saver: PriceSourcesSaver,
        rx_core: broadcast::Receiver<ExchangeEvent>,
        cancellation_token: CancellationToken,
    ) {
        let receiver = self
            .convert_currency_notification_receiver
            .lock()
            .take()
            .expect("PriceSourceEventLoop::convert_currency_notification_receiver is none");

        PriceSourceEventLoop::run(
            self.price_source_chains.values().cloned().collect_vec(),
            price_sources_saver,
            rx_core,
            receiver,
            cancellation_token,
        )
        .await;
    }

    pub fn prepare_price_source_chains(
        price_source_settings: &[CurrencyPriceSourceSettings],
        currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>,
    ) -> Vec<PriceSourceChain> {
        if price_source_settings.is_empty() {
            panic!("price_source_settings shouldn't be empty");
        }

        price_source_settings
            .iter()
            .map(|setting| {
                if setting.start_currency_code == setting.end_currency_code {
                    return PriceSourceChain::new(
                        setting.start_currency_code,
                        setting.end_currency_code,
                        Vec::<RebasePriceStep>::new(),
                    );
                }

                let mut symbol_by_currency_code = HashMap::new();
                for pair in &setting.exchange_id_currency_pair_settings {
                    let symbol = currency_pair_to_symbol_converter
                        .get_symbol(pair.exchange_account_id, pair.currency_pair);
                    Self::add_symbol_to_hashmap(
                        symbol.quote_currency_code(),
                        pair.exchange_account_id.exchange_id,
                        symbol.clone(),
                        &mut symbol_by_currency_code,
                    );
                    Self::add_symbol_to_hashmap(
                        symbol.base_currency_code(),
                        pair.exchange_account_id.exchange_id,
                        symbol.clone(),
                        &mut symbol_by_currency_code,
                    );
                }

                let mut rebase_price_steps = Vec::new();
                let mut current_currency_code = setting.start_currency_code;

                for _ in 0..setting.exchange_id_currency_pair_settings.len() {
                    let list = symbol_by_currency_code
                        .get(&current_currency_code)
                        .with_expect(||
                            Self::format_panic_message(
                                setting,
                                format_args!(
                                    "Can't find currency pair for currency {}",
                                    current_currency_code
                                ),
                            ),
                        );

                    if list.len() > 1 {
                        panic!("{}", Self::format_panic_message(
                            setting,
                            format_args! { "There are more than 1 symbol in the list for currency {}",
                            current_currency_code}
                        ));
                    }

                    let step = list.first().expect("List is empty");

                    rebase_price_steps.push(step.clone());

                    current_currency_code = match step.direction {
                        RebaseDirection::ToQuote => step.symbol.quote_currency_code,
                        RebaseDirection::ToBase => step.symbol.base_currency_code,
                    };

                    if current_currency_code == setting.end_currency_code {
                        break;
                    }
                    let step_symbol = step.symbol.clone();
                    symbol_by_currency_code
                        .get_mut(&current_currency_code)
                        .with_expect(||
                            Self::format_panic_message(
                                setting,
                                format_args!(
                                    "Can't find currency pair for currency {}",
                                    current_currency_code
                                ),
                            ),
                        )
                        .retain(|x| x.symbol != step_symbol);
                }
                PriceSourceChain::new(
                    setting.start_currency_code,
                    setting.end_currency_code,
                    rebase_price_steps,
                )
            })
            .collect_vec()
    }

    fn format_panic_message(
        setting: &CurrencyPriceSourceSettings,
        reason: fmt::Arguments,
    ) -> String {
        format! {"Can't build correct chain of currency pairs of price sources for {}/{} {}",
            setting.start_currency_code, setting.end_currency_code, reason
        }
    }

    fn add_symbol_to_hashmap(
        currency_code: CurrencyCode,
        exchange_id: ExchangeId,
        symbol: Arc<Symbol>,
        symbol_by_currency_code: &mut HashMap<CurrencyCode, Vec<RebasePriceStep>>,
    ) {
        let list = symbol_by_currency_code.entry(currency_code).or_default();
        let direction = match currency_code == symbol.base_currency_code() {
            true => RebaseDirection::ToQuote,
            false => RebaseDirection::ToBase,
        };
        list.push(RebasePriceStep::new(exchange_id, symbol, direction));
    }

    /// Convert amount from 'from' currency position to 'to' currency by current price
    /// Return converted amount or None if can't calculate price for converting and Err if something bad was happened
    pub async fn convert_amount(
        &self,
        from: CurrencyCode,
        to: CurrencyCode,
        src_amount: Amount,
        cancellation_token: CancellationToken,
    ) -> Result<Option<Amount>> {
        let convert_currency_direction = ConvertCurrencyDirection::new(from, to);

        let chain = self
            .price_source_chains
            .get(&convert_currency_direction)
            .context(format!(
                "Failed to get price_sources_chain from {:?} with {:?}",
                self.price_source_chains, convert_currency_direction,
            ))?;

        let (tx_result, rx_result) = oneshot::channel();
        if let Err(error) = self
            .tx_main
            .send(ConvertAmount::new(chain.clone(), src_amount, tx_result))
            .await
        {
            let message = format!(
                "PriceSourceService::convert_amount(): Unable to send: {:?}. Channel is closed",
                error
            );
            if !cancellation_token.is_cancellation_requested() {
                panic!("{} but cancellation hasn't been requested", message);
            }
            log::warn!("{}.", message);
        }

        tokio::select! {
            result = rx_result => Ok(result.context("While receiving the result on rx_result in PriceSourceService::convert_amount()")?),
            _ = cancellation_token.when_cancelled() => Ok(None),
        }
    }

    pub async fn convert_amount_in_past(
        &self,
        from: CurrencyCode,
        to: CurrencyCode,
        src_amount: Amount,
        time_in_past: DateTime,
        cancellation_token: CancellationToken,
    ) -> Option<Amount> {
        let price_sources = self
            .price_sources_loader
            .load(time_in_past, cancellation_token.clone())
            .await
            .with_expect(|| {
                format!(
                    "Failed to get price_sources for {} from database",
                    time_in_past
                )
            });

        let convert_currency_direction = ConvertCurrencyDirection::new(from, to);

        let prices_source_chain = self
            .price_source_chains
            .get(&convert_currency_direction)
            .with_expect(|| {
                format!(
                    "Failed to get price_source_chain for {:?} from {:?}",
                    convert_currency_direction, self.price_source_chains
                )
            });
        prices_calculator::convert_amount_in_past(
            src_amount,
            &price_sources,
            time_in_past,
            prices_source_chain,
        )
    }
}

#[derive(Debug)]
pub struct ConvertAmount {
    pub chain: PriceSourceChain,
    pub src_amount: Amount,
    pub task_finished_sender: oneshot::Sender<Option<Decimal>>,
}

impl ConvertAmount {
    pub fn new(
        chain: PriceSourceChain,
        src_amount: Amount,
        task_finished_sender: oneshot::Sender<Option<Decimal>>,
    ) -> Self {
        Self {
            chain,
            src_amount,
            task_finished_sender,
        }
    }
}

#[cfg(test)]
pub mod test {
    use domain::exchanges::symbol::Precision;
    use domain::market::CurrencyPair;
    use domain::market::ExchangeAccountId;
    use rstest::rstest;
    use rust_decimal_macros::dec;

    use crate::{
        exchanges::general::test_helper::{
            get_test_exchange_by_currency_codes, get_test_exchange_with_symbol,
        },
        settings::ExchangeIdCurrencyPairSettings,
    };

    use super::*;

    pub(crate) struct PriceSourceServiceTestBase {}

    impl PriceSourceServiceTestBase {
        pub fn exchange_id() -> ExchangeId {
            ExchangeId::new("Binance")
        }

        pub fn exchange_account_id() -> ExchangeAccountId {
            ExchangeAccountId::new(PriceSourceServiceTestBase::exchange_id(), 0)
        }

        pub fn exchange_account_id_2() -> ExchangeAccountId {
            ExchangeAccountId::new(PriceSourceServiceTestBase::exchange_id(), 1)
        }

        pub fn exchange_account_id_3() -> ExchangeAccountId {
            ExchangeAccountId::new(PriceSourceServiceTestBase::exchange_id(), 2)
        }
    }

    fn create_symbol(base: CurrencyCode, quote: CurrencyCode) -> Arc<Symbol> {
        Arc::new(Symbol::new(
            false,
            base.as_str().into(),
            base,
            quote.as_str().into(),
            quote,
            None,
            None,
            None,
            None,
            None,
            base,
            None,
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(0) },
        ))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn when_start_equal_end() {
        let usdt = "USDT".into();
        let price_source_settings = vec![CurrencyPriceSourceSettings::new(usdt, usdt, Vec::new())];

        let (converter, _locker) = CurrencyPairToSymbolConverter::init_mock();

        // Act
        let actual = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );

        // Assert
        let expected = PriceSourceChain::new(usdt, usdt, Vec::new());

        assert_eq!(actual.first().expect("in test"), &expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn when_start_equal_end_and_currency_pair() {
        let usdt = "USDT".into();
        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            usdt,
            usdt,
            vec![ExchangeIdCurrencyPairSettings {
                exchange_account_id: PriceSourceServiceTestBase::exchange_account_id(),
                currency_pair: CurrencyPair::from_codes(usdt, usdt),
            }],
        )];

        let (converter, _locker) = CurrencyPairToSymbolConverter::init_mock();

        // Act
        let actual = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );

        // Assert
        let expected = PriceSourceChain::new(usdt, usdt, Vec::new());

        assert_eq!(actual.first().expect("in test"), &expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn when_one_currency_pair() {
        let base = "USDT".into();
        let quote = "BTC".into();
        let currency_pair = CurrencyPair::from_codes(base, quote);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            quote,
            base,
            vec![ExchangeIdCurrencyPairSettings {
                exchange_account_id: PriceSourceServiceTestBase::exchange_account_id(),
                currency_pair,
            }],
        )];

        let symbol = create_symbol(base, quote);

        let symbol_cloned = symbol.clone();
        let (mut converter, _locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_with_symbol(symbol_cloned.clone())
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

        // Act
        let actual = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );

        // Assert
        let expected = PriceSourceChain::new(
            quote,
            base,
            vec![RebasePriceStep::new(
                PriceSourceServiceTestBase::exchange_id(),
                symbol,
                RebaseDirection::ToBase,
            )],
        );

        assert_eq!(actual.first().expect("in test"), &expected);
    }

    #[rstest]
    #[case("EOS".into(), "BTC".into(), "BTC".into(), "USDT".into(), RebaseDirection::ToQuote, RebaseDirection::ToQuote)] // eos_sell_btc_sell_usdt
    #[case("EOS".into(), "BTC".into(), "USDT".into(), "BTC".into(), RebaseDirection::ToQuote, RebaseDirection::ToBase)] // eos_sell_btc_buy_usdt
    #[case("BTC".into(), "EOS".into(), "BTC".into(), "USDT".into(), RebaseDirection::ToBase, RebaseDirection::ToQuote)] // eos_buy_btc_sell_usdt
    #[case("BTC".into(), "EOS".into(), "USDT".into(), "BTC".into(), RebaseDirection::ToBase, RebaseDirection::ToBase)] // eos_buy_btc_buy_usdt
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn when_two_currency_pairs(
        #[case] first_currency: CurrencyCode,
        #[case] second_currency: CurrencyCode,
        #[case] third_currency: CurrencyCode,
        #[case] fourth_currency: CurrencyCode,
        #[case] first_pair_direction: RebaseDirection,
        #[case] second_pair_direction: RebaseDirection,
    ) {
        let currency_pair_1 = CurrencyPair::from_codes(first_currency, second_currency);
        let currency_pair_2 = CurrencyPair::from_codes(third_currency, fourth_currency);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            "EOS".into(),
            "USDT".into(),
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

        let symbol_1 = create_symbol(first_currency, second_currency);
        let symbol_2 = create_symbol(third_currency, fourth_currency);

        let symbol_1_cloned = symbol_1.clone();
        let symbol_2_cloned = symbol_2.clone();
        let (mut converter, _locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_with_symbol(symbol_1_cloned.clone())
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_2()
                {
                    get_test_exchange_with_symbol(symbol_2_cloned.clone())
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

        // Act
        let actual = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );

        // Assert
        let expected = PriceSourceChain::new(
            "EOS".into(),
            "USDT".into(),
            vec![
                RebasePriceStep::new(
                    PriceSourceServiceTestBase::exchange_id(),
                    symbol_1,
                    first_pair_direction,
                ),
                RebasePriceStep::new(
                    PriceSourceServiceTestBase::exchange_id(),
                    symbol_2,
                    second_pair_direction,
                ),
            ],
        );

        assert_eq!(actual.first().expect("in test"), &expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn when_three_currency_pairs_karma_sell_eos_buy_btc_sell_usdt() {
        let eos = "EOS".into();
        let btc = "BTC".into();
        let usdt = "USDT".into();
        let karma = "KARMA".into();
        let currency_pair_1 = CurrencyPair::from_codes(btc, eos);
        let currency_pair_2 = CurrencyPair::from_codes(karma, eos);
        let currency_pair_3 = CurrencyPair::from_codes(btc, usdt);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            karma,
            usdt,
            vec![
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id(),
                    currency_pair: currency_pair_1,
                },
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id_2(),
                    currency_pair: currency_pair_2,
                },
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id_3(),
                    currency_pair: currency_pair_3,
                },
            ],
        )];

        let symbol_1 = create_symbol(btc, eos);
        let symbol_2 = create_symbol(btc, usdt);
        let symbol_3 = create_symbol(karma, eos);

        let symbol_1_cloned = symbol_1.clone();
        let symbol_2_cloned = symbol_2.clone();
        let symbol_3_cloned = symbol_3.clone();
        let (mut converter, _locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_with_symbol(symbol_1_cloned.clone())
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_3()
                {
                    get_test_exchange_with_symbol(symbol_2_cloned.clone())
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_2()
                {
                    get_test_exchange_with_symbol(symbol_3_cloned.clone())
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

        // Act
        let actual = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );

        // Assert
        let expected = PriceSourceChain::new(
            karma,
            usdt,
            vec![
                RebasePriceStep::new(
                    PriceSourceServiceTestBase::exchange_id(),
                    symbol_3,
                    RebaseDirection::ToQuote,
                ),
                RebasePriceStep::new(
                    PriceSourceServiceTestBase::exchange_id(),
                    symbol_1,
                    RebaseDirection::ToBase,
                ),
                RebasePriceStep::new(
                    PriceSourceServiceTestBase::exchange_id(),
                    symbol_2,
                    RebaseDirection::ToQuote,
                ),
            ],
        );

        assert_eq!(actual.first().expect("in test"), &expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "failed to get currency pair")]
    async fn throw_exception_when_more_currencies_then_needed() {
        let eos = "EOS".into();
        let btc = "BTC".into();
        let usdt = "USDT".into();
        let karma = "KARMA".into();
        let currency_pair_1 = CurrencyPair::from_codes(btc, eos);
        let currency_pair_2 = CurrencyPair::from_codes(karma, eos);
        let currency_pair_3 = CurrencyPair::from_codes(btc, usdt);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            eos,
            usdt,
            vec![
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id(),
                    currency_pair: currency_pair_1,
                },
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id_2(),
                    currency_pair: currency_pair_2,
                },
                ExchangeIdCurrencyPairSettings {
                    exchange_account_id: PriceSourceServiceTestBase::exchange_account_id_3(),
                    currency_pair: currency_pair_3,
                },
            ],
        )];

        let (mut converter, _locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_by_currency_codes(false, btc.as_str(), eos.as_str())
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_3()
                {
                    get_test_exchange_by_currency_codes(false, btc.as_str(), usdt.as_str())
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_2()
                {
                    get_test_exchange_by_currency_codes(false, btc.as_str(), karma.as_str())
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

        let _ = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(
        expected = "Can't build correct chain of currency pairs of price sources for karma/usdt"
    )]
    async fn throw_exception_when_less_currencies_then_needed() {
        let eos = "EOS".into();
        let btc = "BTC".into();
        let usdt = "USDT".into();
        let karma = "KARMA".into();
        let currency_pair_1 = CurrencyPair::from_codes(btc, eos);
        let currency_pair_2 = CurrencyPair::from_codes(btc, usdt);

        let price_source_settings = vec![CurrencyPriceSourceSettings::new(
            karma,
            usdt,
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

        let (mut converter, _locker) = CurrencyPairToSymbolConverter::init_mock();
        converter
            .expect_get_symbol()
            .returning(move |exchange_account_id, currency_pair| {
                if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id() {
                    get_test_exchange_by_currency_codes(false, btc.as_str(), eos.as_str())
                } else if exchange_account_id == PriceSourceServiceTestBase::exchange_account_id_2()
                {
                    get_test_exchange_by_currency_codes(false, btc.as_str(), usdt.as_str())
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

        let _ = PriceSourceService::prepare_price_source_chains(
            &price_source_settings,
            Arc::new(converter),
        );
    }
}
