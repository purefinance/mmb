use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

use crate::core::{
    exchanges::{
        common::{Amount, CurrencyCode, ExchangeAccountId, ExchangeId, TradePlace},
        events::ExchangeEvent,
        general::{
            currency_pair_metadata::CurrencyPairMetadata,
            currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter,
        },
    },
    infrastructure::{spawn_future, WithExpect},
    lifecycle::cancellation_token::CancellationToken,
    misc::price_by_order_side::PriceByOrderSide,
    order_book::local_snapshot_service::LocalSnapshotsService,
    services::usd_converter::{prices_calculator, rebase_price_step::RebaseDirection},
    settings::CurrencyPriceSourceSettings,
    DateTime,
};

use anyhow::{bail, Context, Result};
use futures::FutureExt;
use itertools::Itertools;
use parking_lot::Mutex;
use rust_decimal::Decimal;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{
    convert_currency_direction::ConvertCurrencyDirection, price_source_chain::PriceSourceChain,
    price_sources_loader::PriceSourcesLoader, prices_sources_saver::PriceSourcesSaver,
    rebase_price_step::RebasePriceStep,
};

pub struct PriceSourceEventLoop {
    currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
    price_sources_saver: PriceSourcesSaver,
    all_trade_places: HashSet<TradePlace>,
    local_snapshot_service: LocalSnapshotsService,
    price_cache: HashMap<TradePlace, PriceByOrderSide>,
    rx_core: broadcast::Receiver<ExchangeEvent>,
    rx_event_loop: mpsc::Receiver<ConvertAmount>,
}

impl PriceSourceEventLoop {
    pub async fn run(
        currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
        price_source_chains: Vec<PriceSourceChain>,
        price_sources_saver: PriceSourcesSaver,
        rx_core: broadcast::Receiver<ExchangeEvent>,
        rx_event_loop: mpsc::Receiver<ConvertAmount>,
        cancellation_token: CancellationToken,
    ) {
        let run_action = async move {
            let mut this = Self {
                currency_pair_to_metadata_converter,
                price_sources_saver,
                all_trade_places: Self::map_to_used_trade_places(price_source_chains),
                local_snapshot_service: LocalSnapshotsService::new(HashMap::new()),
                price_cache: HashMap::new(),
                rx_core,
                rx_event_loop,
            };
            this.run_loop(cancellation_token).await
        };
        spawn_future("PriceSourceService", true, run_action.boxed())
            .await
            .expect("Failed to spawn PriceSourceService::run_loop() future");
    }

    async fn run_loop(&mut self, cancellation_token: CancellationToken) -> Result<()> {
        loop {
            tokio::select! {
                main_event_res = self.rx_event_loop.recv() => {
                   let convert_amount = main_event_res.context("Error during receiving event on rx_event_loop")?;

                    let result = prices_calculator::convert_amount(
                        convert_amount.src_amount,
                        &self.local_snapshot_service,
                        &convert_amount.chain,
                    );
                    convert_amount.task_finished_sender.send(result).expect("PriceSourceEventLoop::run_loop(): Unable to send trades event. Probably receiver is already dropped");
                },
                core_event_res = self.rx_core.recv() => {
                    let event = core_event_res.context("Error during receiving event on rx_core")?;
                    match event {
                        ExchangeEvent::OrderBookEvent(order_book_event) => {
                            let trade_place = TradePlace::new(
                                order_book_event.exchange_account_id.exchange_id.clone(),
                                order_book_event.currency_pair.clone(),
                            );
                            if self.all_trade_places.contains(&trade_place) {
                                let _ = self.local_snapshot_service.update(order_book_event);
                                self.update_cache_and_save(trade_place);
                            }
                        },
                        _ => continue,
                    }
                }
                _ = cancellation_token.when_cancelled() => bail!("main_loop has been stopped by CancellationToken"),
            };
        }
    }

    fn try_update_cache(&mut self, trade_place: TradePlace, new_value: PriceByOrderSide) -> bool {
        if let Some(old_value) = self.price_cache.get_mut(&trade_place) {
            match old_value == &new_value {
                true => return false,
                false => {
                    *old_value = new_value;
                    return true;
                }
            }
        };

        self.price_cache.insert(trade_place, new_value);
        return true;
    }

    fn update_cache_and_save(&mut self, trade_place: TradePlace) {
        let snapshot = self
            .local_snapshot_service
            .get_snapshot(&trade_place)
            .with_expect(|| {
                format!(
                    "Can't get snapshot for {:?} (this shouldn't happen)",
                    trade_place
                )
            });

        let price_by_order_side = snapshot.get_top_prices();
        if self.try_update_cache(trade_place.clone(), price_by_order_side.clone()) {
            self.price_sources_saver
                .save(trade_place, price_by_order_side);
        }
    }

    fn map_to_used_trade_places(price_source_chains: Vec<PriceSourceChain>) -> HashSet<TradePlace> {
        price_source_chains
            .into_iter()
            .flat_map(|price_source_chain| {
                price_source_chain
                    .rebase_price_steps
                    .into_iter()
                    .map(|step| {
                        TradePlace::new(
                            step.exchange_id,
                            step.currency_pair_metadata.currency_pair(),
                        )
                    })
            })
            .collect()
    }
}

pub struct PriceSourceService {
    price_sources_loader: PriceSourcesLoader,
    tx_main: mpsc::Sender<ConvertAmount>,
    rx_event_loop: Mutex<Option<mpsc::Receiver<ConvertAmount>>>,
    price_source_chains: HashMap<ConvertCurrencyDirection, PriceSourceChain>,
}

impl PriceSourceService {
    pub fn new(
        currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
        price_source_settings: &Vec<CurrencyPriceSourceSettings>,
        price_sources_loader: PriceSourcesLoader,
    ) -> Arc<Self> {
        let price_source_chains = Self::prepare_price_source_chains(
            price_source_settings,
            currency_pair_to_metadata_converter.clone(),
        );
        let (tx_main, rx_event_loop) = mpsc::channel(20_000);

        Arc::new(Self {
            price_sources_loader,
            tx_main,
            rx_event_loop: Mutex::new(Some(rx_event_loop)),
            price_source_chains: price_source_chains
                .into_iter()
                .map(|x| {
                    (
                        ConvertCurrencyDirection::new(
                            x.start_currency_code.clone(),
                            x.end_currency_code.clone(),
                        ),
                        x,
                    )
                })
                .collect(),
        })
    }
    pub async fn start(
        self: Arc<Self>,
        currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
        price_sources_saver: PriceSourcesSaver,
        rx_core: broadcast::Receiver<ExchangeEvent>,
        cancellation_token: CancellationToken,
    ) {
        PriceSourceEventLoop::run(
            currency_pair_to_metadata_converter,
            self.price_source_chains.values().cloned().collect_vec(),
            price_sources_saver,
            rx_core,
            self.rx_event_loop
                .lock()
                .take()
                .expect("Failed to run PriceSourceEventLoop rx_event_loop is none"),
            cancellation_token,
        )
        .await;
    }

    pub fn prepare_price_source_chains(
        price_source_settings: &Vec<CurrencyPriceSourceSettings>,
        currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
    ) -> Vec<PriceSourceChain> {
        if price_source_settings.is_empty() {
            panic!("price_source_settings shouldn't be empty");
        }

        price_source_settings
            .iter()
            .map(|setting| {
                if setting.start_currency_code == setting.end_currency_code {
                    return PriceSourceChain::new(
                        setting.start_currency_code.clone(),
                        setting.end_currency_code.clone(),
                        Vec::<RebasePriceStep>::new(),
                    );
                }

                let mut currency_pair_metadata_by_currency_code = HashMap::new();
                for pair in &setting.exchange_id_currency_pair_settings {
                    let metadata = currency_pair_to_metadata_converter
                        .get_currency_pair_metadata(&pair.exchange_account_id, &pair.currency_pair);
                    Self::add_currency_pair_metadata_to_hashmap(
                        &metadata.quote_currency_code(),
                        pair.exchange_account_id.exchange_id.clone(),
                        metadata.clone(),
                        &mut currency_pair_metadata_by_currency_code,
                    );
                    Self::add_currency_pair_metadata_to_hashmap(
                        &metadata.base_currency_code(),
                        pair.exchange_account_id.exchange_id.clone(),
                        metadata.clone(),
                        &mut currency_pair_metadata_by_currency_code,
                    );
                }

                let mut rebase_price_steps = Vec::new();
                let mut current_currency_code = setting.start_currency_code.clone();

                for _ in 0..setting.exchange_id_currency_pair_settings.len() {
                    let list = currency_pair_metadata_by_currency_code
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
                        RebaseDirection::ToQuote => step.currency_pair_metadata.quote_currency_code.clone(),
                        RebaseDirection::ToBase => step.currency_pair_metadata.base_currency_code.clone(),
                    };

                    if current_currency_code == setting.end_currency_code {
                        break;
                    }
                    let step_metadata = step.currency_pair_metadata.clone();
                    currency_pair_metadata_by_currency_code
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
                        .retain(|x| x.currency_pair_metadata != step_metadata);
                }
                PriceSourceChain::new(
                    setting.start_currency_code.clone(),
                    setting.end_currency_code.clone(),
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

    fn add_currency_pair_metadata_to_hashmap(
        currency_code: &CurrencyCode,
        exchange_id: ExchangeId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_pair_metadata_by_currency_code: &mut HashMap<CurrencyCode, Vec<RebasePriceStep>>,
    ) {
        let list = currency_pair_metadata_by_currency_code
            .entry(currency_code.clone())
            .or_default();
        let direction = match currency_code == &currency_pair_metadata.base_currency_code() {
            true => RebaseDirection::ToQuote,
            false => RebaseDirection::ToBase,
        };
        list.push(RebasePriceStep::new(
            exchange_id,
            currency_pair_metadata,
            direction,
        ));
    }

    /// Convert amount from 'from' currency position to 'to' currency by current price
    /// Return converted amount or None if can't calculate price for converting and Err if something bad was happened
    pub async fn convert_amount(
        &self,
        from: &CurrencyCode,
        to: &CurrencyCode,
        src_amount: Amount,
        cancellation_token: CancellationToken,
    ) -> Result<Option<Amount>> {
        let convert_currency_direction = ConvertCurrencyDirection::new(from.clone(), to.clone());

        let chain = self
            .price_source_chains
            .get(&convert_currency_direction)
            .context(format!(
                "Failed to get price_sources_chain from {:?} with {:?}",
                self.price_source_chains, convert_currency_direction,
            ))?;

        let (tx_result, rx_result) = oneshot::channel();
        self
            .tx_main
            .send(ConvertAmount::new(chain.clone(), src_amount, tx_result))
            .await
            .expect(
                "PriceSourceService::convert_amount(): Unable to send trades event. Probably receiver is already dropped"
            );
        tokio::select! {
            result = rx_result => Ok(result.context("While receiving the result on rx_result in PriceSourceService::convert_amount()")?),
            _ = cancellation_token.when_cancelled() => Ok(None),
        }
    }

    pub async fn convert_amount_in_past(
        &self,
        from: &CurrencyCode,
        to: &CurrencyCode,
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

        let convert_currency_direction = ConvertCurrencyDirection::new(from.clone(), to.clone());

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

pub mod test {
    use super::*;

    pub(crate) struct PriceSourceServiceTestBase {}

    impl PriceSourceServiceTestBase {
        pub fn get_exchange_account_id() -> ExchangeAccountId {
            ExchangeAccountId::new(PriceSourceServiceTestBase::get_exchange_id(), 0)
        }

        pub fn get_exchange_account_id_2() -> ExchangeAccountId {
            ExchangeAccountId::new(PriceSourceServiceTestBase::get_exchange_id(), 1)
        }

        pub fn get_exchange_id() -> ExchangeId {
            ExchangeId::new("Binance".into())
        }
    }
}
