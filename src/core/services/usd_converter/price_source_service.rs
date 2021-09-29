use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::core::{
    exchanges::{
        common::{Amount, CurrencyCode, ExchangeAccountId, ExchangeId, TradePlace},
        general::{
            currency_pair_metadata::CurrencyPairMetadata,
            currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter,
            exchange::Exchange,
        },
    },
    lifecycle::{application_manager::ApplicationManager, cancellation_token::CancellationToken},
    misc::price_by_order_side::PriceByOrderSide,
    order_book::local_snapshot_service::LocalSnapshotsService,
    settings::engine::price_source::CurrencyPriceSourceSettings,
};

use anyhow::Result;
use itertools::Itertools;

use super::{
    convert_currency_direction::ConvertCurrencyDirection, price_source_chain::PriceSourceChain,
    price_sources_loader::PriceSourcesLoader, prices_sources_saver::PriceSourcesSaver,
    rebase_price_step::RebasePriceStep,
};
pub struct PriceSourceService {
    currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter,
    // private readonly IPricesCalculator _pricesCalculator;
    price_sources_saver: PriceSourcesSaver,
    price_sources_loader: PriceSourcesLoader,
    // private readonly EventsChannelAsync<IExchangeEvent> _coreChannel;
    // private readonly EventsChannelAsync<PriceSourceServiceEvent> _mainChannel;
    price_sources_chains: HashMap<ConvertCurrencyDirection, PriceSourceChain>,
    local_snapshot_service: LocalSnapshotsService,
    all_trade_places: HashSet<TradePlace>,
    price_cache: HashMap<TradePlace, PriceByOrderSide>,
}

impl PriceSourceService {
    // pub fn new(
    //     exchanges_by_id: HashMap<ExchangeAccountId, Exchange>,
    //     currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter,

    //     // EventsChannelAsync<IExchangeEvent> coreChannel,
    //     price_source_settings: Vec<CurrencyPriceSourceSettings>,
    //     // IPricesCalculator pricesCalculator,
    //     price_sources_saver: PriceSourcesSaver,
    //     price_sources_loader: PriceSourcesLoader,
    //     application_manager: Arc<ApplicationManager>,
    // ) -> Self {
    //     Self {
    //         currency_pair_to_metadata_converter,
    //         price_sources_saver,
    //         price_sources_loader,
    //         price_sources_chains,
    //         local_snapshot_service: LocalSnapshotsService::new(HashMap::new()),
    //         all_trade_places,
    //         price_cache,
    //     }
    // }

    // _localSnapshotService = new LocalSnapshotService(exchangesById);
    // _priceCache = new Dictionary<ExchangeNameSymbol, PricesBySide>();

    // _coreChannel = coreChannel;
    // _mainChannel = new EventsChannelAsync<PriceSourceServiceEvent>(
    //     $"{nameof(PriceSourceService)}_MainLoop",
    //     dateTimeService,
    //     applicationManager);

    // var priceSourceChains =
    //     PreparePriceSourceChains(priceSourceSettings, exchangesById, currencyPairToSymbolConverter);
    // _priceSourceChains = priceSourceChains
    //     .ToDictionary(x => new ConvertCurrencyDirection(x.StartCurrencyCode, x.EndCurrencyCode));
    // _allEns = GetUsedEns(priceSourceChains);
    // }

    pub fn prepare_price_source_chains(
        price_source_settings: Vec<CurrencyPriceSourceSettings>,
        currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter,
    ) -> Vec<PriceSourceChain> {
        if price_source_settings.is_empty() {
            std::panic!("price_source_settings shouldn't be empty");
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
                for pair in &setting.exchange_id_currency_code_pair_settings {
                    let metadata = currency_pair_to_metadata_converter
                        .get_currency_pair_metadata(&pair.exchange_account_id, &pair.currency_pair);
                    PriceSourceService::add_currency_pair_metadata_to_hashmap(
                        &metadata.quote_currency_code(),
                        pair.exchange_account_id.exchange_id.clone(),
                        metadata.clone(),
                        &mut currency_pair_metadata_by_currency_code,
                    );
                    PriceSourceService::add_currency_pair_metadata_to_hashmap(
                        &metadata.base_currency_code(),
                        pair.exchange_account_id.exchange_id.clone(),
                        metadata.clone(),
                        &mut currency_pair_metadata_by_currency_code,
                    );
                }

                let mut rebase_price_steps = Vec::new();
                let mut current_currency_code = setting.start_currency_code.clone();

                for _ in 0..setting.exchange_id_currency_code_pair_settings.len() {
                    let list = currency_pair_metadata_by_currency_code
                        .get(&current_currency_code)
                        .expect(
                            PriceSourceService::format_panic_message(
                                setting,
                                format!(
                                    "Can't find currency pair for currency {}",
                                    current_currency_code
                                ),
                            )
                            .as_str(),
                        );

                    if list.len() > 1 {
                        std::panic!(PriceSourceService::format_panic_message(
                            setting,
                            format! { "There are more than 1 symbol in the list for currency {}",
                            current_currency_code}
                        ));
                    }

                    let step = list.first().expect("list is empty");

                    rebase_price_steps.push(step.clone());

                    current_currency_code = match step.from_base_to_quote_currency {
                        true => step.currency_pair_metadata.quote_currency_code.clone(),
                        false => step.currency_pair_metadata.base_currency_code.clone(),
                    };

                    if current_currency_code == setting.end_currency_code {
                        break;
                    }
                    let step_metadata = step.currency_pair_metadata.clone();
                    currency_pair_metadata_by_currency_code
                        .get_mut(&current_currency_code)
                        .expect(
                            PriceSourceService::format_panic_message(
                                setting,
                                format!(
                                    "Can't find currency pair for currency {}",
                                    current_currency_code
                                ),
                            )
                            .as_str(),
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

    fn format_panic_message(setting: &CurrencyPriceSourceSettings, reason: String) -> String {
        format! {"Can't build correct chain of currency pairs of price sources for {}/{} {}",
            setting.start_currency_code, setting.end_currency_code, reason
        }
    }

    fn get_used_trade_places(price_source_chains: Vec<PriceSourceChain>) -> HashSet<TradePlace> {
        price_source_chains
            .iter()
            .flat_map(|price_source_chain| {
                // let b: HashSet<TradePlace> =
                price_source_chain.rebase_price_steps.iter().map(|step| {
                    TradePlace::new(
                        step.exchange_id.clone(),
                        step.currency_pair_metadata.currency_pair(),
                    )
                })
            })
            .collect()
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
        let is_base = currency_code == &currency_pair_metadata.base_currency_code();
        list.push(RebasePriceStep::new(
            exchange_id,
            currency_pair_metadata,
            is_base,
        ));
    }

    pub async fn convert_amount(
        &self,
        from_currency_code: &CurrencyCode,
        to_currency_code: &CurrencyCode,
        src_amount: Amount,
        cancellation_token: CancellationToken,
    ) -> Result<Option<Amount>> {
        //TODO: should be implemented
        Ok(None)
    }
    //     /// <summary>
    //     /// Convert amount from <see cref="fromCurrencyCode"/> currency position to <see cref="toCurrencyCode"/> currency by current price
    //     /// </summary>
    //     /// <returns>
    //     /// Return converted amount or null if can't calculate price for converting
    //     /// </returns>
    //     public async Task<decimal?> ConvertAmount(
    //         string fromCurrencyCode,
    //         string toCurrencyCode,
    //         decimal sourceAmount,
    //         CancellationToken cancellationToken)
    //     {
    //         var tcs = new TaskCompletionSource<decimal?>(TaskCreationOptions.RunContinuationsAsynchronously);
    //         var convertCurrencyDirection = new ConvertCurrencyDirection(fromCurrencyCode, toCurrencyCode);
    //         _mainChannel.AddEvent(new PriceSourceService_ConvertAmountNow(convertCurrencyDirection, sourceAmount, tcs));

    //         await using (cancellationToken.Register(() => tcs.SetCanceled()))
    //         {
    //             return await tcs.Task;
    //         }
    //     }

    //     public async Task<decimal?> ConvertAmountInPast(
    //         string fromCurrencyCode,
    //         string toCurrencyCode,
    //         decimal sourceAmount,
    //         DateTime timeInPast,
    //         CancellationToken cancellationToken)
    //     {
    //         var priceSources = await _priceSourcesLoader.Load(timeInPast, cancellationToken);
    //         if (priceSources == null || priceSources.Count == 0)
    //         {
    //             return null;
    //         }

    //         var convertCurrencyDirection = new ConvertCurrencyDirection(fromCurrencyCode, toCurrencyCode);
    //         return _pricesCalculator.ConvertAmountInPast(
    //             sourceAmount,
    //             priceSources,
    //             timeInPast,
    //             _priceSourceChains[convertCurrencyDirection]);
    //     }

    //     private ExchangeNameSymbol GetEns(string exchangeId, string exchangeName, string currencyPair)
    //     {
    //         var symbol = _currencyPairToSymbolConverter.GetSymbol(exchangeId, currencyPair);
    //         return new ExchangeNameSymbol(exchangeName, symbol.CurrencyCodePair);
    //     }

    fn try_update_cache(&mut self, trade_place: TradePlace, new_value: &PriceByOrderSide) -> bool {
        let value = self
            .price_cache
            .entry(trade_place)
            .or_insert(new_value.clone());

        match value == new_value {
            true => {
                *value = new_value.clone();
                false
            }
            false => false,
        }
    }

    //     private void UpdateCacheAndSave(ExchangeNameSymbol ens)
    //     {
    //         if (!_localSnapshotService.TryGetSnapshot(ens, out var snapshot))
    //         {
    //             throw new Exception($"Can't get snapshot for {ens} (this shouldn't happen)");
    //         }

    //         var pricesBySide = snapshot.CalculatePrice().IntoPricesBySide();
    //         if (TryUpdateCache(ens, pricesBySide))
    //         {
    //             _priceSourcesSaver.Save(ens, pricesBySide);
    //         }
    //     }

    //     public async Task Start(CancellationToken cancellationToken)
    //     {
    //         var coreLoopTask = Task.Run(() => FromCoreLoop(cancellationToken), cancellationToken);
    //         var mainLoopTask = Task.Run(() => MainLoop(cancellationToken), cancellationToken);
    //         await Task.WhenAll(coreLoopTask, mainLoopTask);
    //     }

    //     private async Task FromCoreLoop(CancellationToken cancellationToken)
    //     {
    //         await EventLoop.RunSimple(_coreChannel, cancellationToken, newEvent =>
    //         {
    //             if (newEvent is OrderBookEvent orderBookEvent)
    //             {
    //                 _mainChannel.AddEvent(new PriceSourceService_OrderBookEvent(orderBookEvent));
    //             }
    //         });
    //     }

    //     private async Task MainLoop(CancellationToken cancellationToken)
    //     {
    //         await EventLoop.RunSimple(_mainChannel, cancellationToken, newEvent =>
    //         {
    //             switch (newEvent)
    //             {
    //                 case PriceSourceService_ConvertAmountNow convertAmountNow:
    //                 {
    //                     try
    //                     {
    //                         var result = _pricesCalculator.ConvertAmountNow(
    //                             convertAmountNow.SourceAmount,
    //                             _localSnapshotService,
    //                             _priceSourceChains[convertAmountNow.ConvertCurrencyDirection]);
    //                         convertAmountNow.Tcs.TrySetResult(result);
    //                     }
    //                     catch (Exception ex)
    //                     {
    //                         convertAmountNow.Tcs.TrySetException(ex);
    //                     }
    //                     break;
    //                 }

    //                 case PriceSourceService_OrderBookEvent orderBookEvent:
    //                 {
    //                     var snapshot = orderBookEvent.OrderBookEvent;
    //                     var ens = GetEns(snapshot.ExchangeId, snapshot.ExchangeName, snapshot.CurrencyPair);
    //                     if (_allEns.Contains(ens))
    //                     {
    //                         _localSnapshotService.Update(snapshot);
    //                         UpdateCacheAndSave(ens);
    //                     }
    //                     break;
    //                 }

    //                 default:
    //                     throw new ArgumentOutOfRangeException(nameof(newEvent), newEvent, "Unsupported event");
    //             }
    //         });
    //     }

    //     #region PriceSourceServiceEvents

    //     // ReSharper disable InconsistentNaming

    //     private abstract class PriceSourceServiceEvent
    //     {
    //     }

    //     private class PriceSourceService_OrderBookEvent : PriceSourceServiceEvent
    //     {
    //         public OrderBookEvent OrderBookEvent { get; }

    //         public PriceSourceService_OrderBookEvent(OrderBookEvent orderBookEvent)
    //         {
    //             OrderBookEvent = orderBookEvent;
    //         }
    //     }

    //     private class PriceSourceService_ConvertAmountNow : PriceSourceServiceEvent
    //     {
    //         public ConvertCurrencyDirection ConvertCurrencyDirection { get; }

    //         public decimal SourceAmount { get; }

    //         public TaskCompletionSource<decimal?> Tcs { get; }

    //         public PriceSourceService_ConvertAmountNow(
    //             ConvertCurrencyDirection convertCurrencyDirection,
    //             decimal sourceAmount,
    //             TaskCompletionSource<decimal?> tcs)
    //         {
    //             ConvertCurrencyDirection = convertCurrencyDirection;
    //             SourceAmount = sourceAmount;
    //             Tcs = tcs;
    //         }
    //     }

    //     // ReSharper restore InconsistentNaming

    //     #endregion
    // }
}
