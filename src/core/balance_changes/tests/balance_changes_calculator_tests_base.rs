#[cfg(test)]
pub mod tests {

    use std::{collections::HashMap, sync::Arc};

    use mockall_double::double;
    use parking_lot::{Mutex, MutexGuard, RwLock};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[double]
    use crate::core::exchanges::general::currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter;
    #[double]
    use crate::core::misc::time_manager::time_manager;
    #[double]
    use crate::core::services::usd_converter::usd_converter::UsdConverter;

    use crate::{
        core::{
            balance_changes::{
                balance_change_calculator_result::BalanceChangesCalculatorResult,
                balance_changes_calculator::BalanceChangesCalculator,
                profit_balance_changes_calculator,
                profit_loss_balance_change::ProfitLossBalanceChange,
            },
            balance_manager::balance_request::BalanceRequest,
            exchanges::{
                common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price},
                general::{
                    currency_pair_metadata::{CurrencyPairMetadata, Precision},
                    exchange::Exchange,
                    test_helper::get_test_exchange_by_currency_codes,
                },
            },
            lifecycle::cancellation_token::CancellationToken,
            orders::{
                fill::{EventSourceType, OrderFill, OrderFillType},
                order::{
                    ClientOrderFillId, ClientOrderId, OrderExecutionType, OrderFillRole,
                    OrderHeader, OrderRole, OrderSide, OrderSnapshot, OrderType, ReservationId,
                },
                pool::OrderRef,
            },
            service_configuration::configuration_descriptor::ConfigurationDescriptor,
        },
        hashmap,
    };

    pub struct BalanceChangesCalculatorTestsBase {
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        pub currency_list: Vec<CurrencyCode>,
        pub exchange_1: Arc<Exchange>,
        pub exchange_2: Arc<Exchange>,
        pub exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
        pub currency_pair_to_metadata_converter: Arc<CurrencyPairToMetadataConverter>,
        balance_changes: Vec<BalanceChangesCalculatorResult>,
        balance_changes_calculator: BalanceChangesCalculator,
        profit_loss_balance_changes: Vec<ProfitLossBalanceChange>,
        usd_converter: UsdConverter,

        time_manager_mock: time_manager::__now::Context,
        seconds_offset: Arc<Mutex<u32>>,
        mock_lockers: Vec<MutexGuard<'static, ()>>,
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

        pub fn init_usd_converter(
            prices: HashMap<CurrencyCode, Price>,
        ) -> (UsdConverter, MutexGuard<'static, ()>) {
            let (mut usd_converter, usd_converter_locker) = UsdConverter::init_mock();
            usd_converter
                .expect_convert_amount()
                .returning(move |from, amount, _| {
                    if *from == Self::quote() {
                        return Some(amount);
                    }

                    let price = prices.get(&from).expect("in test").clone();
                    Some(amount * price)
                });
            (usd_converter, usd_converter_locker)
        }

        fn init_currency_pair_to_metadata_converter(
            exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
        ) -> (CurrencyPairToMetadataConverter, MutexGuard<'static, ()>) {
            let (mut currency_pair_to_metadata_converter, cp_to_metadata_locker) =
                CurrencyPairToMetadataConverter::init_mock();
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
            currency_pair_to_metadata_converter
                .expect_get_currency_pair_metadata()
                .returning(move |_, _| metadata.clone());

            // TODO: grays maybe need to delete
            //     CurrencyPairToSymbolConverter.Setup(x => x.GetSymbolByCurrencyCodePair(It.IsAny<string>(), CurrencyPair)).Returns(symbol);

            currency_pair_to_metadata_converter
                .expect_exchanges_by_id()
                .returning(move || exchanges_by_id.clone());

            (currency_pair_to_metadata_converter, cp_to_metadata_locker)
        }

        pub fn set_leverage(&mut self, leverage: Decimal) {
            self.exchange_1
                .leverage_by_currency_pair
                .insert(Self::currency_pair(), leverage);
            self.exchange_2
                .leverage_by_currency_pair
                .insert(Self::currency_pair(), leverage);
        }

        pub fn new() -> Self {
            let exchange_1 = get_test_exchange_by_currency_codes(
                false,
                &Self::base().as_str(),
                &Self::quote().as_str(),
            )
            .0;
            let exchange_2 = get_test_exchange_by_currency_codes(
                false,
                &Self::base().as_str(),
                &Self::quote().as_str(),
            )
            .0;

            let exchanges_by_id = hashmap![
                Self::exchange_account_id_1() => exchange_1.clone(),
                Self::exchange_account_id_2() => exchange_2.clone()
            ];

            let mut mock_lockers = Vec::new();

            let (currency_pair_to_metadata_converter, cp_to_metadata_locker) =
                Self::init_currency_pair_to_metadata_converter(exchanges_by_id.clone());
            let currency_pair_to_metadata_converter = Arc::new(currency_pair_to_metadata_converter);
            mock_lockers.push(cp_to_metadata_locker);

            let (usd_converter, usd_converter_locker) = Self::init_usd_converter(hashmap![
                Self::base() => dec!(1000),
                Self::quote() => dec!(1)
            ]);
            mock_lockers.push(usd_converter_locker);

            let seconds_offset = Arc::new(Mutex::new(0u32));
            let (time_manager_mock, time_manager_locker) =
                crate::core::misc::time_manager::tests::init_mock(seconds_offset.clone());
            mock_lockers.push(time_manager_locker);

            let mut this = Self {
                configuration_descriptor: Arc::new(ConfigurationDescriptor::new(
                    Self::service_name(),
                    Self::service_configuration_key(),
                )),
                currency_list: vec![Self::base(), Self::quote()],
                exchange_1,
                exchange_2,
                exchanges_by_id,
                currency_pair_to_metadata_converter: currency_pair_to_metadata_converter.clone(),
                balance_changes: Vec::new(),
                balance_changes_calculator: BalanceChangesCalculator::new(
                    currency_pair_to_metadata_converter,
                ),
                profit_loss_balance_changes: Vec::new(),
                usd_converter,
                time_manager_mock,
                seconds_offset,
                mock_lockers,
            };

            this.set_leverage(dec!(1));

            this
        }

        pub fn create_order_with_commission_amount(
            exchange_account_id: ExchangeAccountId,
            currency_pair: CurrencyPair,
            trade_side: OrderSide,
            price: Price,
            amount: Amount,
            filled_amount: Amount,
            commission_currency_code: CurrencyCode,
            commission_amount: Amount,
        ) -> OrderRef // TODO: grays maybe ORderRef
        {
            let mut order = OrderSnapshot::with_params(
                ClientOrderId::unique_id(),
                OrderType::Limit,
                None,
                exchange_account_id,
                currency_pair,
                price,
                amount,
                trade_side,
                None,
                "in test",
            );

            if filled_amount > dec!(0) {
                order.add_fill(OrderFill::new(
                    Uuid::nil(),
                    None,
                    time_manager::now(),
                    OrderFillType::UserTrade,
                    None,
                    price,
                    filled_amount,
                    dec!(0),
                    OrderFillRole::Maker,
                    commission_currency_code.clone(),
                    commission_amount,
                    dec!(0),
                    commission_currency_code,
                    commission_amount,
                    commission_amount,
                    true,
                    None,
                    None,
                ));
            }
            OrderRef::new(Arc::new(RwLock::new(order)))
        }

        pub async fn calculate_balance_changes(&mut self, orders: Vec<&OrderRef>) {
            for order in orders {
                for fill in order.get_fills().0 {
                    let balance_changes = self.balance_changes_calculator.get_balance_changes(
                        self.configuration_descriptor.clone(),
                        order,
                        fill,
                    );
                    for (request, balance_change) in
                        balance_changes.get_changes().get_as_balances().into_iter()
                    {
                        let usd_change = balance_changes
                            .calculate_usd_change(
                                request.currency_code.clone(),
                                balance_change,
                                &self.usd_converter,
                                CancellationToken::default(),
                            )
                            .await;
                        let profit_loss_balance_change = ProfitLossBalanceChange::new(
                            request,
                            order.exchange_account_id().exchange_id,
                            ClientOrderFillId::unique_id(),
                            time_manager::now(),
                            balance_change,
                            usd_change,
                        );
                    }
                    self.balance_changes.push(balance_changes);
                }
            }
        }

        pub fn get_actual_balance_change(
            &self,
            exchange_account_id: ExchangeAccountId,
            currency_pair: CurrencyPair,
            currency_code: CurrencyCode,
        ) -> Decimal {
            let request = BalanceRequest::new(
                self.configuration_descriptor.clone(),
                exchange_account_id,
                currency_pair,
                currency_code,
            );

            self.balance_changes
                .iter()
                .map(|x| {
                    x.get_changes()
                        .get_by_balance_request(&request)
                        .unwrap_or(dec!(0))
                })
                .sum()
        }

        pub fn calculate_raw_profit(&self) -> Decimal {
            profit_balance_changes_calculator::calculate_raw(&self.profit_loss_balance_changes)
        }

        pub async fn calculate_over_market_profit(&self) -> Decimal {
            profit_balance_changes_calculator::calculate_over_market(
                &self.profit_loss_balance_changes,
                &self.usd_converter,
                CancellationToken::default(),
            )
            .await
        }
    }
}
