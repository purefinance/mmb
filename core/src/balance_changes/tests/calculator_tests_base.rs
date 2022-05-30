#[cfg(test)]
pub mod tests {

    use std::{collections::HashMap, sync::Arc};

    use mmb_utils::cancellation_token::CancellationToken;
    use mmb_utils::hashmap;
    use mockall_double::double;
    use parking_lot::{Mutex, ReentrantMutexGuard};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[double]
    use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
    #[double]
    use crate::misc::time::time_manager;
    #[double]
    use crate::services::usd_convertion::usd_converter::UsdConverter;

    use crate::infrastructure::init_lifetime_manager;
    use crate::misc::time;
    use crate::service_configuration::configuration_descriptor::{
        ServiceConfigurationKey, ServiceName,
    };
    use crate::{
        balance_changes::{
            balance_change_calculator_result::BalanceChangesCalculatorResult,
            balance_changes_calculator::BalanceChangesCalculator,
            profit_balance_changes_calculator, profit_loss_balance_change::ProfitLossBalanceChange,
        },
        balance_manager::balance_request::BalanceRequest,
        exchanges::{
            common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price},
            general::{
                exchange::Exchange,
                symbol::{Precision, Symbol},
                test_helper::get_test_exchange_by_currency_codes,
            },
        },
        orders::{
            fill::{OrderFill, OrderFillType},
            order::{
                ClientOrderFillId, ClientOrderId, OrderFillRole, OrderSide, OrderSnapshot,
                OrderType,
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
        pub currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>,
        balance_changes: Vec<BalanceChangesCalculatorResult>,
        balance_changes_calculator: BalanceChangesCalculator,
        profit_loss_balance_changes: Vec<ProfitLossBalanceChange>,
        pub usd_converter: UsdConverter,

        _time_manager_mock: time_manager::__now::Context,
        _seconds_offset: Arc<Mutex<u32>>,
        _mock_lockers: Vec<ReentrantMutexGuard<'static, ()>>,
    }

    impl BalanceChangesCalculatorTestsBase {
        pub fn commission_rate_1() -> Decimal {
            dec!(0.01)
        }

        pub fn exchange_account_id_1() -> ExchangeAccountId {
            ExchangeAccountId::new("EXC1", 0)
        }

        pub fn exchange_account_id_2() -> ExchangeAccountId {
            ExchangeAccountId::new("EXC2", 0)
        }

        pub fn base() -> CurrencyCode {
            "BTC".into()
        }

        pub fn quote() -> CurrencyCode {
            "USD".into()
        }

        pub fn currency_pair() -> CurrencyPair {
            CurrencyPair::from_codes(Self::base(), Self::quote())
        }

        fn service_name() -> ServiceName {
            "calculator_tests_base_service_name".into()
        }

        fn service_configuration_key() -> ServiceConfigurationKey {
            "calculator_tests_base_service_key".into()
        }

        pub fn amount_multiplier() -> Decimal {
            dec!(0.001)
        }

        pub fn init_usd_converter(
            prices: HashMap<CurrencyCode, Price>,
        ) -> (UsdConverter, ReentrantMutexGuard<'static, ()>) {
            let (mut usd_converter, usd_converter_locker) = UsdConverter::init_mock();
            usd_converter
                .expect_convert_amount()
                .returning(move |from, amount, _| {
                    if from == Self::quote() {
                        return Some(amount);
                    }

                    let price = prices.get(&from).expect("in test").clone();
                    Some(amount * price)
                });
            (usd_converter, usd_converter_locker)
        }

        fn init_currency_pair_to_symbol_converter(
            exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
            is_derivative: bool,
            is_reversed: bool,
        ) -> (
            CurrencyPairToSymbolConverter,
            ReentrantMutexGuard<'static, ()>,
        ) {
            let (mut currency_pair_to_symbol_converter, cp_to_symbol_locker) =
                CurrencyPairToSymbolConverter::init_mock();

            let (amount_currency_code, balance_currency_code) = match (is_derivative, is_reversed) {
                (true, true) => (Self::base(), Some(Self::quote())),
                (true, false) => (Self::quote(), Some(Self::base())),
                (false, true) => todo!("This combo doesn't use anywhere now"),
                (false, false) => (Self::base(), None),
            };

            let mut symbol = Symbol::new(
                false,
                is_derivative,
                Self::base().as_str().into(),
                Self::base(),
                Self::quote().as_str().into(),
                Self::quote(),
                None,
                None,
                None,
                None,
                None,
                amount_currency_code,
                balance_currency_code,
                Precision::ByTick { tick: dec!(0.1) },
                Precision::ByTick { tick: dec!(0) },
            );
            if is_reversed {
                symbol.amount_multiplier = Self::amount_multiplier();
            }
            let symbol = Arc::new(symbol);

            currency_pair_to_symbol_converter
                .expect_get_symbol()
                .returning(move |_, _| symbol.clone());

            currency_pair_to_symbol_converter
                .expect_exchanges_by_id()
                .return_const(exchanges_by_id);

            (currency_pair_to_symbol_converter, cp_to_symbol_locker)
        }

        pub fn set_leverage(&mut self, leverage: Decimal) {
            self.exchange_1
                .leverage_by_currency_pair
                .insert(Self::currency_pair(), leverage);
            self.exchange_2
                .leverage_by_currency_pair
                .insert(Self::currency_pair(), leverage);
        }

        pub fn new(is_derivative: bool, is_reversed: bool) -> Self {
            init_lifetime_manager();

            let (usd_converter, usd_converter_locker) = Self::init_usd_converter(hashmap![
                Self::base() => dec!(1000),
                Self::quote() => dec!(1)
            ]);

            Self::new_with_usd_converter(
                is_derivative,
                is_reversed,
                usd_converter,
                usd_converter_locker,
            )
        }

        pub fn new_with_usd_converter(
            is_derivative: bool,
            is_reversed: bool,
            usd_converter: UsdConverter,
            usd_converter_locker: ReentrantMutexGuard<'static, ()>,
        ) -> Self {
            let exchange_1 = get_test_exchange_by_currency_codes(
                false,
                Self::base().as_str(),
                Self::quote().as_str(),
            )
            .0;
            let exchange_2 = get_test_exchange_by_currency_codes(
                false,
                Self::base().as_str(),
                Self::quote().as_str(),
            )
            .0;

            let exchanges_by_id = hashmap![
                Self::exchange_account_id_1() => exchange_1.clone(),
                Self::exchange_account_id_2() => exchange_2.clone()
            ];

            let mut mock_lockers = Vec::new();

            let (currency_pair_to_symbol_converter, cp_to_symbol_locker) =
                Self::init_currency_pair_to_symbol_converter(
                    exchanges_by_id.clone(),
                    is_derivative,
                    is_reversed,
                );
            let currency_pair_to_symbol_converter = Arc::new(currency_pair_to_symbol_converter);
            mock_lockers.push(cp_to_symbol_locker);

            mock_lockers.push(usd_converter_locker);

            let seconds_offset = Arc::new(Mutex::new(0u32));
            let (time_manager_mock, time_manager_locker) =
                time::tests::init_mock(seconds_offset.clone());
            mock_lockers.push(time_manager_locker);

            let mut this = Self {
                configuration_descriptor: ConfigurationDescriptor::new(
                    Self::service_name(),
                    Self::service_configuration_key(),
                ),
                currency_list: vec![Self::base(), Self::quote()],
                exchange_1,
                exchange_2,
                exchanges_by_id,
                currency_pair_to_symbol_converter: currency_pair_to_symbol_converter.clone(),
                balance_changes: Vec::new(),
                balance_changes_calculator: BalanceChangesCalculator::new(
                    currency_pair_to_symbol_converter,
                ),
                profit_loss_balance_changes: Vec::new(),
                usd_converter,
                _time_manager_mock: time_manager_mock,
                _seconds_offset: seconds_offset,
                _mock_lockers: mock_lockers,
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
        ) -> OrderSnapshot {
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
                    commission_currency_code,
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

            order
        }

        pub async fn calculate_balance_changes(&mut self, orders: Vec<&OrderSnapshot>) {
            for order in orders {
                for fill in &order.fills.fills {
                    let balance_changes = self.balance_changes_calculator.get_balance_changes(
                        self.configuration_descriptor,
                        order,
                        fill,
                    );
                    for (request, balance_change) in
                        balance_changes.get_changes().get_as_balances().into_iter()
                    {
                        let usd_change = balance_changes
                            .calculate_usd_change(
                                request.currency_code,
                                balance_change,
                                &self.usd_converter,
                                CancellationToken::default(),
                            )
                            .await;

                        let profit_loss_balance_change = ProfitLossBalanceChange::new(
                            request,
                            order.header.exchange_account_id.exchange_id,
                            ClientOrderFillId::unique_id(),
                            time_manager::now(),
                            balance_change,
                            usd_change,
                        );
                        self.profit_loss_balance_changes
                            .push(profit_loss_balance_change);
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
                self.configuration_descriptor,
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
