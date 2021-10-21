#[cfg(test)]
pub mod tests {

    use std::{collections::HashMap, sync::Arc};

    use mockall_double::double;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    #[double]
    use crate::core::exchanges::general::currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter;
    #[double]
    use crate::core::services::usd_converter::usd_converter::UsdConverter;

    use crate::core::{
        balance_changes::{
            balance_change_calculator_result::BalanceChangesCalculatorResult,
            profit_loss_balance_change::ProfitLossBalanceChange,
        },
        exchanges::{
            common::{CurrencyCode, CurrencyPair, ExchangeAccountId, Price},
            general::{
                currency_pair_metadata::{CurrencyPairMetadata, Precision},
                exchange::Exchange,
            },
        },
        service_configuration::configuration_descriptor::ConfigurationDescriptor,
    };

    pub struct BalanceChangesCalculatorTestsBase {
        configuration_descriptor: ConfigurationDescriptor,
        pub currency_list: Vec<CurrencyCode>,
        pub exchange_1: Arc<Exchange>,
        pub exchange_2: Arc<Exchange>,
        pub exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
        pub currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter,
        balance_changes: Vec<BalanceChangesCalculatorResult>,
        profit_loss_balance_changes: Vec<ProfitLossBalanceChange>,
        usd_converter: UsdConverter,
    }

    impl BalanceChangesCalculatorTestsBase {
        pub fn commission_rate_1() -> Decimal {
            dec!(0.01)
        }

        pub fn commission_rate_2() -> Decimal {
            dec!(0.02)
        }

        pub fn exchange_account_id_1() -> ExchangeAccountId {
            ExchangeAccountId::new("EXC1".into(), 0)
        }

        pub fn exchange_account_id_2() -> ExchangeAccountId {
            ExchangeAccountId::new("EXC2".into(), 0)
        }

        pub fn base() -> CurrencyCode {
            "BTC".into()
        }

        pub fn quote() -> CurrencyCode {
            "USD".into()
        }

        pub fn currency_pair() -> CurrencyPair {
            CurrencyPair::from_codes(&Self::base(), &Self::quote())
        }

        pub fn inverted_currency_pair() -> CurrencyPair {
            CurrencyPair::from_codes(&Self::quote(), &Self::base())
        }

        fn service_name() -> String {
            "name".into()
        }

        fn service_configuration_key() -> String {
            "key".into()
        }

        pub fn init_usd_converter(&mut self, prices: HashMap<CurrencyCode, Price>) {
            self.usd_converter
                .expect_convert_amount()
                .returning(move |from, amount, _| {
                    if *from == Self::quote() {
                        return Some(amount);
                    }

                    let price = prices.get(&from).expect("in test").clone();
                    Some(amount * price)
                });
        }

        // private void InitExchangesMocks()
        // {
        //     Exchange1 = new Mock<Exchange>();
        //     Exchange2 = new Mock<Exchange>();

        //     CurrencyList = new []{BaseCurrencyCode, QuoteCurrencyCode};
        //     Exchange1.Setup(x => x.Currencies).Returns(CurrencyList);
        //     Exchange2.Setup(x => x.Currencies).Returns(CurrencyList);

        //     Symbol symbol = new Symbol(false, BaseCurrencyCode, QuoteCurrencyCode, CurrencyPair);
        //     var symbols = new List<Symbol>{symbol};
        //     Exchange1.Setup(x => x.Symbols).Returns(symbols);
        //     Exchange2.Setup(x => x.Symbols).Returns(symbols);

        //     SetLeverage(1m);
        // }

        // protected void SetLeverage(decimal leverage)
        // {
        //     var leverageByCurrencyPair = new Dictionary<string, decimal>
        //     {
        //         [CurrencyPair] = leverage
        //     };

        //     Exchange1.Object.LeverageByCurrencyPair = leverageByCurrencyPair;
        //     Exchange2.Object.LeverageByCurrencyPair = leverageByCurrencyPair;
        // }

        // private void InitExchangesById()
        // {
        //     ExchangesById.TryAdd(ExchangeId1, Exchange1.Object);
        //     ExchangesById.TryAdd(ExchangeId2, Exchange2.Object);
        // }

        fn init_currency_pair_to_metadata_converter(&mut self) {
            let metadata = Arc::new(CurrencyPairMetadata::new(
                false,
                false,
                Self::base().as_str().into(),
                Self::base(),
                Self::quote().as_str().into(),
                Self::quote(),
                None,
                None,
                None,
                None,
                None,
                Self::base().into(),
                None,
                Precision::ByTick { tick: dec!(0.1) },
                Precision::ByTick { tick: dec!(0) },
            ));
            self.currency_pair_to_metadata_converter
                .expect_get_currency_pair_metadata()
                .returning(move |_, _| metadata.clone());

            // TODO: grays maybe need to delete
            //     CurrencyPairToSymbolConverter.Setup(x => x.GetSymbolByCurrencyCodePair(It.IsAny<string>(), CurrencyPair)).Returns(symbol);

            let exchanges_by_id = self.exchanges_by_id.clone();
            self.currency_pair_to_metadata_converter
                .expect_exchanges_by_id()
                .returning(move || exchanges_by_id.clone());
        }

        // [SetUp]
        // public void Setup()
        // {
        //     BalanceChanges = new List<BalanceChangesCalculatorResult>();
        //     ProfitLossBalanceChanges = new List<ProfitLossBalanceChange>();

        //     ExchangesById = new ConcurrentDictionary<string, Exchange>();
        //     CurrencyPairToSymbolConverter = new Mock<ICurrencyPairToSymbolConverter>();
        //     InitExchangesMocks();
        //     InitExchangesById();
        //     InitCurrencyPairToSymbolConverter();
        //     InitUsdConverter(new Dictionary<string, decimal>
        //     {
        //         [BaseCurrencyCode] = 1_000,
        //         [QuoteCurrencyCode] = 1
        //     });

        //     ConfigurationDescriptor = new ConfigurationDescriptor(ServiceName, ServiceConfigurationKey);
        //     DateTimeService = new DateTimeService();
        //     BalanceChangesCalculator = new BalanceChangesCalculator(CurrencyPairToSymbolConverter.Object);
        // }

        // protected static Order CreateOrderWithCommissionAmount(
        //     string exchangeId,
        //     string currencyPair,
        //     TradeSide tradeSide,
        //     decimal price,
        //     decimal amount,
        //     decimal filledAmount,
        //     string commissionCurrencyCode,
        //     decimal commissionAmount)
        // {
        //     var order = new Order(
        //         default,
        //         null,
        //         exchangeId,
        //         null,
        //         currencyPair,
        //         null,
        //         tradeSide == TradeSide.Buy ? OrderSide.Buy : OrderSide.Sell,
        //         price,
        //         amount,
        //         OrderType.Limit,
        //         null,
        //         -1,
        //         null)
        //     {
        //         FilledAmount = filledAmount
        //     };

        //     if (filledAmount > 0)
        //     {
        //         order.AddFill(
        //             new OrderFill(
        //                 null,
        //                 null,
        //                 price,
        //                 filledAmount,
        //                 0,
        //                 true,
        //                 commissionCurrencyCode,
        //                 commissionAmount,
        //                 0,
        //                 commissionCurrencyCode,
        //                 commissionAmount,
        //                 commissionAmount,
        //                 null,
        //                 true,
        //                 default,
        //                 default,
        //                 default,
        //                 default
        //             )
        //         );
        //     }

        //     return order;
        // }

        // protected async Task CalculateBalanceChanges(params Order[] orders)
        // {
        //     foreach (var order in orders)
        //     {
        //         foreach (var fill in order.GetFills())
        //         {
        //             var balanceChanges = BalanceChangesCalculator.GetBalanceChanges(ConfigurationDescriptor, order, fill);
        //             var changes = balanceChanges.GetChanges();
        //             foreach (var (request, balanceChange) in changes.GetAsBalances())
        //             {
        //                 var usdChange = await balanceChanges.CalculateUsdChange(
        //                     request.CurrencyCode, balanceChange, _usdConverter);
        //                 var profitLossBalanceChange = new ProfitLossBalanceChange(request, order.ExchangeName, null, DateTime.UtcNow, balanceChange, usdChange);
        //                 ProfitLossBalanceChanges.Add(profitLossBalanceChange);
        //             }

        //             BalanceChanges.Add(balanceChanges);
        //         }
        //     }
        // }

        // protected decimal? GetActualBalanceChange(string exchangeId, string currencyPair, string currencyCode)
        // {
        //     var request = new BalanceRequest(ConfigurationDescriptor, exchangeId, currencyPair, currencyCode);

        //     var resChange = 0m;
        //     foreach (var calculatorResult in BalanceChanges)
        //     {
        //         var change = calculatorResult.GetChanges().Get(request);
        //         if (change != null)
        //         {
        //             resChange += change.Value;
        //         }
        //     }

        //     return resChange;
        // }

        // protected decimal CalculateRawProfit()
        // {
        //     var calculator = new ProfitBalanceChangesCalculator();
        //     return calculator.CalculateRaw(ProfitLossBalanceChanges);
        // }

        // protected Task<decimal> CalculateOverMarketProfit()
        // {
        //     var calculator = new ProfitBalanceChangesCalculator();
        //     return calculator.CalculateOverMarket(ProfitLossBalanceChanges, _usdConverter, CancellationToken.None);
        // }
    }
}
