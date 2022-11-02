use mmb_utils::hashmap;
use mmb_utils::DateTime;
use mockall_double::double;
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use mmb_domain::exchanges::symbol::{Precision, Symbol};
use mmb_domain::market::ExchangeAccountId;
use mmb_domain::order::fill::{OrderFill, OrderFillType};
use mmb_domain::order::snapshot::OrderFillRole;
use mmb_domain::order::snapshot::{Amount, Price};
use std::collections::HashMap;
use std::sync::Arc;

use crate::balance::manager::tests::balance_manager_base::BalanceManagerBase;
#[double]
use crate::misc::time::time_manager;
use crate::{
    balance::manager::balance_manager::BalanceManager,
    exchanges::general::{
        currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter, exchange::Exchange,
        test_helper::get_test_exchange_with_symbol_and_id,
    },
};

pub struct BalanceManagerOrdinal {
    pub balance_manager_base: BalanceManagerBase,
    pub now: DateTime,
}

impl BalanceManagerOrdinal {
    pub fn create_balance_manager() -> (Arc<Symbol>, Arc<Mutex<BalanceManager>>) {
        let (symbol, exchanges_by_id) =
            BalanceManagerOrdinal::create_balance_manager_ctor_parameters();
        let currency_pair_to_symbol_converter = CurrencyPairToSymbolConverter::new(exchanges_by_id);

        let balance_manager = BalanceManager::new(currency_pair_to_symbol_converter, None);
        (symbol, balance_manager)
    }

    fn create_balance_manager_ctor_parameters(
    ) -> (Arc<Symbol>, HashMap<ExchangeAccountId, Arc<Exchange>>) {
        let base = BalanceManagerBase::eth();
        let quote = BalanceManagerBase::btc();
        let symbol = Arc::from(Symbol::new(
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
            Some(quote),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(0.001) },
        ));

        let exchange_1 = get_test_exchange_with_symbol_and_id(
            symbol.clone(),
            ExchangeAccountId::new(BalanceManagerBase::exchange_id().as_str(), 0),
        )
        .0;
        let exchange_2 = get_test_exchange_with_symbol_and_id(
            symbol.clone(),
            ExchangeAccountId::new(BalanceManagerBase::exchange_id().as_str(), 1),
        )
        .0;

        let res = hashmap![
            exchange_1.exchange_account_id => exchange_1,
            exchange_2.exchange_account_id => exchange_2
        ];
        (symbol, res)
    }

    fn new() -> Self {
        let (symbol, balance_manager) = BalanceManagerOrdinal::create_balance_manager();
        let mut balance_manager_base = BalanceManagerBase::new();
        balance_manager_base.set_balance_manager(balance_manager);
        balance_manager_base.set_symbol(symbol);
        let now = time_manager::now();

        Self {
            balance_manager_base,
            now,
        }
    }

    fn create_order_fill(price: Price, amount: Amount, cost: Decimal) -> OrderFill {
        BalanceManagerOrdinal::create_order_fill_with_time(price, amount, cost, time_manager::now())
    }

    fn create_order_fill_with_time(
        price: Price,
        amount: Amount,
        cost: Decimal,
        receive_time: DateTime,
    ) -> OrderFill {
        OrderFill::new(
            Uuid::new_v4(),
            None,
            receive_time,
            OrderFillType::UserTrade,
            None,
            price,
            amount,
            cost,
            OrderFillRole::Taker,
            BalanceManagerBase::bnb(),
            dec!(0.1),
            dec!(0),
            BalanceManagerBase::bnb(),
            dec!(0.1),
            dec!(0.1),
            false,
            None,
            None,
        )
    }

    pub fn balance_manager(&self) -> MutexGuard<BalanceManager> {
        self.balance_manager_base.balance_manager()
    }

    fn check_time(&self, _seconds: u32) {
        // TODO: fix me when mock will be added
    }

    fn timer_add_second(&mut self) {
        *self.balance_manager_base.seconds_offset_in_mock.lock() += 1;
    }
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Utc;
    use mmb_domain::market::CurrencyCode;
    use mmb_domain::order::snapshot::{Amount, Price};
    use mmb_utils::hashmap;
    use mmb_utils::logger::init_logger;
    use parking_lot::{Mutex, RwLock};
    use rstest::rstest;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::balance::manager::balance_manager::BalanceManager;
    use crate::balance::manager::position_change::PositionChange;
    use crate::balance::manager::tests::balance_manager_base::BalanceManagerBase;
    use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
    use crate::misc::reserve_parameters::ReserveParameters;
    use mmb_domain::exchanges::symbol::{Precision, Symbol};
    use mmb_domain::market::{ExchangeAccountId, MarketAccountId};
    use mmb_domain::order::pool::OrdersPool;
    use mmb_domain::order::snapshot::{
        ClientOrderFillId, ClientOrderId, OrderSide, OrderSnapshot, OrderStatus, ReservationId,
    };

    use super::BalanceManagerOrdinal;

    fn create_eth_btc_test_obj(btc_amount: Amount, eth_amount: Amount) -> BalanceManagerOrdinal {
        let test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let mut balance_map: HashMap<CurrencyCode, Amount> = HashMap::new();
        let btc_currency_code = BalanceManagerBase::btc();
        let eth_currency_code = BalanceManagerBase::eth();
        balance_map.insert(btc_currency_code, btc_amount);
        balance_map.insert(eth_currency_code, eth_amount);

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );
        test_object
    }

    fn create_test_obj_with_multiple_currencies(
        currency_codes: Vec<CurrencyCode>,
        amounts: Vec<Amount>,
    ) -> BalanceManagerOrdinal {
        if currency_codes.len() != amounts.len() {
            panic!("Failed to create test object: currency_codes.len() = {} should be equal amounts.len() = {}",
            currency_codes.len(), amounts.len());
        }
        let test_object = BalanceManagerOrdinal::new();

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            test_object.balance_manager_base.exchange_account_id_1,
            currency_codes
                .into_iter()
                .zip(amounts.into_iter())
                .collect(),
        );
        test_object
    }

    fn create_eth_btc_test_obj_for_two_exchanges(
        cc_for_first: CurrencyCode,
        amount_for_first: Amount,
        cc_for_second: CurrencyCode,
        amount_for_second: Amount,
    ) -> BalanceManagerOrdinal {
        let test_object = BalanceManagerOrdinal::new();

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            test_object.balance_manager_base.exchange_account_id_1,
            hashmap![cc_for_first => amount_for_first],
        );

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            test_object.balance_manager_base.exchange_account_id_2,
            hashmap![cc_for_second => amount_for_second],
        );
        test_object
    }

    fn create_test_obj_by_currency_code(
        currency_code: CurrencyCode,
        amount: Amount,
    ) -> BalanceManagerOrdinal {
        create_test_obj_by_currency_code_with_limit(currency_code, amount, None)
    }

    fn create_test_obj_by_currency_code_with_limit(
        currency_code: CurrencyCode,
        amount: Amount,
        limit: Option<Amount>,
    ) -> BalanceManagerOrdinal {
        let test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        if let Some(limit) = limit {
            let configuration_descriptor =
                test_object.balance_manager_base.configuration_descriptor;
            let symbol = test_object.balance_manager_base.symbol();

            test_object.balance_manager().set_target_amount_limit(
                configuration_descriptor,
                exchange_account_id,
                symbol,
                limit,
            );
            let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
                OrderSide::Buy,
                dec!(0.2),
                dec!(2),
            );
            assert_eq!(
                test_object
                    .balance_manager()
                    .get_balance_by_reserve_parameters(&reserve_parameters),
                None
            );
        }

        let mut balance_map: HashMap<CurrencyCode, Amount> = HashMap::new();
        balance_map.insert(currency_code, amount);
        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );
        test_object
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn balance_was_received_not_existing_exchange_account_id() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();
        assert!(!test_object
            .balance_manager()
            .balance_was_received(ExchangeAccountId::new("NotExistingExchangeId", 0)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn balance_was_received_existing_exchange_account_id_without_currency() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();
        assert!(!test_object
            .balance_manager()
            .balance_was_received(test_object.balance_manager_base.exchange_account_id_1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn balance_was_received_existing_exchange_account_id_with_currency() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(2));

        assert!(test_object
            .balance_manager()
            .balance_was_received(test_object.balance_manager_base.exchange_account_id_1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn update_exchange_balance_skip_currencies_with_zero_balance_which_are_not_part_of_currency_pairs(
    ) {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let btc = BalanceManagerBase::btc();
        let eth = BalanceManagerBase::eth();
        let bnb = BalanceManagerBase::bnb();
        let eos = "EOS".into();

        let balance_manager = &mut test_object.balance_manager();
        let currencies = hashmap![
            btc => dec!(2),
            eth => dec!(1),
            bnb => dec!(7.5),
            eos => dec!(0)
        ];
        BalanceManagerBase::update_balance(balance_manager, exchange_account_id, currencies);

        let symbol = test_object.balance_manager_base.symbol();
        let btc_balance =
            balance_manager.get_exchange_balance(exchange_account_id, symbol.clone(), btc);
        assert_eq!(btc_balance, Some(dec!(2)));

        assert_eq!(
            balance_manager.get_exchange_balance(exchange_account_id, symbol.clone(), eth),
            Some(dec!(1))
        );

        assert_eq!(
            balance_manager.get_exchange_balance(exchange_account_id, symbol.clone(), bnb),
            Some(dec!(7.5))
        );

        assert_eq!(
            balance_manager.get_exchange_balance(exchange_account_id, symbol, eos),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_buy_returns_quote_balance_and_currency_code() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(0.5), dec!(0.1));
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let side = OrderSide::Buy;

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_reservation_currency_code(
                    exchange_account_id,
                    test_object.balance_manager_base.symbol(),
                    side,
                ),
            BalanceManagerBase::btc()
        );

        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                test_object.balance_manager_base.configuration_descriptor,
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                side,
                dec!(1),
            ),
            Some(dec!(0.5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_sell_return_base_balance_and_currency_code() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(0.5), dec!(0.1));
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let side = OrderSide::Sell;

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_reservation_currency_code(
                    exchange_account_id,
                    test_object.balance_manager_base.symbol(),
                    side,
                ),
            BalanceManagerBase::eth()
        );

        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                test_object.balance_manager_base.configuration_descriptor,
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                side,
                dec!(1),
            ),
            Some(dec!(0.1))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_buy_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_buy_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1.0))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_sell_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_sell_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5.0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(5.0))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_buy_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None,)
            .is_none());
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_buy_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Buy);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_sell_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None,)
            .is_none());
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_sell_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Sell);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_update_reservation_buy_worse_price_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert!(!test_object
            .balance_manager()
            .try_update_reservation(reservation_id, dec!(0.3)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.1))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_update_reservation_buy_worse_price_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert!(test_object
            .balance_manager()
            .try_update_reservation(reservation_id, dec!(0.3)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.price, dec!(0.3));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_update_reservation_buy_better_price() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.1))
        );

        assert!(test_object
            .balance_manager()
            .try_update_reservation(reservation_id, dec!(0.1)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.6))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.price, dec!(0.1));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_update_reservation_sell() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5.0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");
        assert!(test_object
            .balance_manager()
            .try_update_reservation(reservation_id, dec!(0.1)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.price, dec!(0.1));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_pair_not_enough_balance_for_1() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(0.0), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve_pair(reserve_parameters_1.clone(), reserve_parameters_2.clone(),)
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
            Some(dec!(5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_pair_not_enough_balance_for_2() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(3), dec!(0));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve_pair(reserve_parameters_1.clone(), reserve_parameters_2.clone(),)
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(3))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
            Some(dec!(0))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_pair_enough_balance() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(1), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let (reservation_id_1, reservation_id_2) = test_object
            .balance_manager()
            .try_reserve_pair(reserve_parameters_1.clone(), reserve_parameters_2.clone())
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0.0))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
            Some(dec!(0.0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Buy);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Sell);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_three_not_enough_balance_for_1() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(0.0), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(4),
        );

        let reserve_parameters_3 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(1),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve_three(
                reserve_parameters_1.clone(),
                reserve_parameters_2,
                reserve_parameters_3.clone(),
            )
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_three_not_enough_balance_for_2() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(1), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(6),
        );

        let reserve_parameters_3 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(1),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve_three(
                reserve_parameters_1.clone(),
                reserve_parameters_2,
                reserve_parameters_3.clone(),
            )
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(1))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_three_not_enough_balance_for_3() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(1), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_3 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(1),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve_three(
                reserve_parameters_1.clone(),
                reserve_parameters_2,
                reserve_parameters_3.clone(),
            )
            .is_none());
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(1))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(5))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_three_enough_balance() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(1), dec!(6));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_3 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(1),
        );

        let (reservation_id_1, reservation_id_2, reservation_id_3) = test_object
            .balance_manager()
            .try_reserve_three(
                reserve_parameters_1.clone(),
                reserve_parameters_2,
                reserve_parameters_3.clone(),
            )
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(0))
        );

        let balance_manager = test_object.balance_manager();

        if balance_manager.get_reservation(reservation_id_1).is_none()
            || balance_manager.get_reservation(reservation_id_2).is_none()
            || balance_manager.get_reservation(reservation_id_3).is_none()
        {
            panic!();
        }

        let reservation = balance_manager.get_reservation_expected(reservation_id_1);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Buy);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Sell);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());

        let reservation = balance_manager.get_reservation_expected(reservation_id_3);

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.symbol,
            test_object.balance_manager_base.symbol()
        );
        assert_eq!(reservation.order_side, OrderSide::Sell);
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(1));
        assert_eq!(reservation.not_approved_amount, dec!(1));
        assert_eq!(reservation.unreserved_amount, dec!(1));
        assert!(reservation.approved_parts.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_should_not_unreserve_for_unknown_exchange_account_id() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .get_mut_reservation(reservation_id)
            .expect("in test")
            .exchange_account_id = ExchangeAccountId::new("unknown_id", 0);

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(reservation.unreserved_amount, dec!(5));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_can_unreserve_more_than_reserved_with_compensation_amounts() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(5.00001))
            .expect("in test");

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_can_not_unreserve_after_complete_unreserved() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let error = test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(5))
            .expect_err("should be error");

        if !error.to_string().contains("Can't find reservation_id=") {
            panic!("{:?}", error)
        }

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[rstest]
    #[case(dec!(0))]
    // min positive value in rust_decimaL::Decimal (Scale maximum precision - 28)
    #[case(dec!(1e-28))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_zero_amount(#[case] amount_to_unreserve: Amount) {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let symbol = Arc::from(Symbol::new(
            false,
            BalanceManagerBase::eth().as_str().into(),
            BalanceManagerBase::eth(),
            BalanceManagerBase::btc().as_str().into(),
            BalanceManagerBase::btc(),
            None,
            None,
            None,
            None,
            Some(dec!(1)),
            BalanceManagerBase::eth(),
            Some(BalanceManagerBase::btc()),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(1) },
        ));

        let reserve_parameters = ReserveParameters::new(
            test_object.balance_manager_base.configuration_descriptor,
            test_object.balance_manager_base.exchange_account_id_1,
            symbol,
            OrderSide::Sell,
            dec!(0.2),
            dec!(1),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        // TODO: add cehcking that log didn't contain Err lvl messages
        test_object
            .balance_manager()
            .unreserve(reservation_id, amount_to_unreserve)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(4))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(reservation.unreserved_amount, dec!(1));
        assert_eq!(reservation.not_approved_amount, dec!(1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_buy() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.8))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(reservation.unreserved_amount, dec!(1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_sell() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(4))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(reservation.unreserved_amount, dec!(1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_rest_buy() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_rest_sell() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(5))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_rest_partially_unreserved_buy() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_rest_partially_unreserved_sell() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(5))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[rstest]
    #[case(dec!(5), dec!(0.2), dec!(3), dec!(0.5), dec!(2) ,dec!(2) )]
    #[case(dec!(5), dec!(0.2), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservation_different_price_sell(
        #[case] src_balance: Amount,
        #[case] price_1: Price,
        #[case] amount_1: Amount,
        #[case] price_2: Price,
        #[case] amount_2: Amount,
        #[case] amount_to_transfer: Amount,
    ) {
        init_logger();
        let test_object = create_eth_btc_test_obj(src_balance, src_balance);

        let side = OrderSide::Sell;

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(side, price_1, amount_1);
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let balance_1 = src_balance - amount_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_1)
        );

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(side, price_2, amount_2);
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        let balance_2 = balance_1 - amount_2;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
            Some(balance_2)
        );

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            amount_to_transfer,
            &None
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_2 - amount_to_transfer + amount_to_transfer)
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(reservation.cost, dec!(1));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);
        assert_eq!(reservation.cost, dec!(4));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[rstest]
    #[case(dec!(5), dec!(0.2), dec!(3), dec!(0.5), dec!(2) ,dec!(2) )]
    #[case(dec!(5), dec!(0.2), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservation_different_price_buy(
        #[case] src_balance: Amount,
        #[case] price_1: Price,
        #[case] amount_1: Amount,
        #[case] price_2: Price,
        #[case] amount_2: Amount,
        #[case] amount_to_transfer: Amount,
    ) {
        init_logger();
        let test_object = create_eth_btc_test_obj(src_balance, src_balance);

        let side = OrderSide::Buy;

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(side, price_1, amount_1);
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let balance_1 = src_balance - amount_1 * price_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_1)
        );

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(side, price_2, amount_2);
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        let balance_2 = balance_1 - amount_2 * price_2;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
            Some(balance_2)
        );

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            amount_to_transfer,
            &None
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_2 + amount_to_transfer * price_1 - amount_to_transfer * price_2)
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(reservation.cost, dec!(1));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);
        assert_eq!(reservation.cost, dec!(4));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_partial() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );

        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &None
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(reservation.cost, dec!(1));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);
        assert_eq!(reservation.cost, dec!(4));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_all() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );

        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(3),
            &None
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .is_none());

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(reservation.cost, dec!(2) + dec!(3));
        assert_eq!(reservation.amount, dec!(2) + dec!(3));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(3));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(3));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_more_than_we_have_should_do_nothing_and_panic() {
        init_logger();
        let test_object = Arc::new(Mutex::new(create_test_obj_by_currency_code(
            BalanceManagerBase::eth(),
            dec!(5),
        )));

        let reserve_parameters_1 = test_object
            .lock()
            .balance_manager_base
            .create_reserve_parameters(OrderSide::Sell, dec!(0.2), dec!(3));

        let reserve_parameters_2 = test_object
            .lock()
            .balance_manager_base
            .create_reserve_parameters(OrderSide::Sell, dec!(0.2), dec!(2));

        let reservation_id_1 = test_object
            .lock()
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let reservation_id_2 = test_object
            .lock()
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        let balance_manager_cloned = Mutex::new(test_object.lock().balance_manager().clone());

        let handle = std::thread::spawn(move || {
            balance_manager_cloned.lock().try_transfer_reservation(
                reservation_id_1,
                reservation_id_2,
                dec!(5),
                &None,
            );
        });

        if handle.join().is_ok() {
            panic!();
        }

        assert_eq!(
            test_object
                .lock()
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        assert_eq!(
            test_object
                .lock()
                .balance_manager()
                .get_reservation_expected(reservation_id_1)
                .unreserved_amount,
            dec!(3)
        );
        assert_eq!(
            test_object
                .lock()
                .balance_manager()
                .get_reservation_expected(reservation_id_2)
                .unreserved_amount,
            dec!(2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_zero_from_zero_reservation_should_remove_reservation() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(0),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(0))
            .expect("in test");

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_with_unreserve() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id_1, dec!(1))
            .expect("in test");

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(1),
            &None
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(1))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation_expected(reservation_id_1)
                .unreserved_amount,
            dec!(3) - dec!(1) - dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation_expected(reservation_id_2)
                .unreserved_amount,
            dec!(2) + dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_partial_approve() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        let amount = test_object
            .balance_manager()
            .get_reservation_expected(reservation_id_1)
            .amount;
        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order.header.client_order_id,
            amount,
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert!(reservation
            .approved_parts
            .contains_key(&order.header.client_order_id));

        let amount = reservation.amount;
        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            amount
        );

        assert!(balance_manager.try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(order.header.client_order_id.clone())
        ));

        assert_eq!(
            balance_manager.get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(reservation.cost, dec!(3) - dec!(2));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(0));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));
        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            dec!(3) - dec!(2)
        );

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(reservation.cost, dec!(2) + dec!(2));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));

        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            dec!(2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic]
    pub async fn transfer_reservations_amount_more_thane_we_have() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        let amount = test_object
            .balance_manager()
            .get_reservation_expected(reservation_id_1)
            .amount;
        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order.header.client_order_id,
            amount,
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            reservation.amount
        );

        balance_manager.try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(4),
            &None,
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic]
    pub async fn transfer_reservations_amount_more_than_we_have_by_approve_client_order_id() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order.header.client_order_id,
            dec!(1),
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            dec!(1)
        );

        balance_manager.try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(order.header.client_order_id.clone()),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic]
    pub async fn transfer_reservations_unknown_client_order_id() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(ClientOrderId::new("unknown_id".into())),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_partial_approve_with_multiple_orders() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        let order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        let order_2 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order_1.header.client_order_id,
            dec!(1),
        );

        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order_2.header.client_order_id,
            dec!(2),
        );

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(order_2.header.client_order_id.clone()),
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);

        assert_eq!(reservation.cost, dec!(3) - dec!(2));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(0));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        assert!(reservation
            .approved_parts
            .contains_key(&order_1.header.client_order_id));

        assert!(!reservation
            .approved_parts
            .contains_key(&order_2.header.client_order_id));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(reservation.cost, dec!(2) + dec!(2));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));

        assert_eq!(
            reservation
                .approved_parts
                .get(&order_2.header.client_order_id)
                .expect("in test")
                .amount,
            dec!(2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservations_amount_partial_approve_with_multiple_orders_to_existing_part(
    ) {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(2),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        let order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        let order_2 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order_1.header.client_order_id,
            dec!(1),
        );

        test_object.balance_manager().approve_reservation(
            reservation_id_1,
            &order_2.header.client_order_id,
            dec!(2),
        );

        test_object.balance_manager().approve_reservation(
            reservation_id_2,
            &order_2.header.client_order_id,
            dec!(1),
        );

        assert!(test_object.balance_manager().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(order_2.header.client_order_id.clone()),
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);

        assert_eq!(reservation.cost, dec!(3) - dec!(2));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(0));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        assert!(reservation
            .approved_parts
            .contains_key(&order_1.header.client_order_id));

        assert!(!reservation
            .approved_parts
            .contains_key(&order_2.header.client_order_id));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(reservation.cost, dec!(2) + dec!(2));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(1));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));

        assert_eq!(
            reservation
                .approved_parts
                .get(&order_2.header.client_order_id)
                .expect("in test")
                .amount,
            dec!(1) + dec!(2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_pair() {
        init_logger();
        let test_object = create_eth_btc_test_obj_for_two_exchanges(
            BalanceManagerBase::btc(),
            dec!(1),
            BalanceManagerBase::eth(),
            dec!(5),
        );

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        let reserve_parameters_2 = ReserveParameters::new(
            test_object.balance_manager_base.configuration_descriptor,
            test_object.balance_manager_base.exchange_account_id_2,
            test_object.balance_manager_base.symbol(),
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );

        let (reservation_id_1, reservation_id_2) = test_object
            .balance_manager()
            .try_reserve_pair(reserve_parameters_1.clone(), reserve_parameters_2.clone())
            .expect("in test");

        test_object.balance_manager().unreserve_pair(
            reservation_id_1,
            dec!(5),
            reservation_id_2,
            dec!(5),
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(1))
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
            Some(dec!(5))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .is_none());

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_not_existing_exchange_account_id() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                test_object.balance_manager_base.configuration_descriptor,
                ExchangeAccountId::new("unknown_id", 0),
                test_object.balance_manager_base.symbol(),
                OrderSide::Buy,
                dec!(1),
            ),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_not_existing_currency_code() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(2));

        assert_eq!(
            test_object.balance_manager().get_balance_by_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                "not_existing_currency_code".into(),
                dec!(1),
            ),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_unapproved_reservations_are_counted_even_after_balance_update() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let btc = BalanceManagerBase::btc();
        let eth = BalanceManagerBase::eth();
        let bnb = BalanceManagerBase::bnb();

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            hashmap![
                btc => dec!(2),
                eth => dec!(0.5),
                bnb => dec!(0.1)
            ],
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(5),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            hashmap![
                btc => dec!(2),
                eth => dec!(0.5),
                bnb => dec!(0.1)
            ],
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(2) - dec!(0.2) * dec!(5))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                eth
            ),
            Some(dec!(0.5))
        );
        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                bnb
            ),
            Some(dec!(0.1))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_approved_reservations_are_not_counted_after_balance_update() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let btc = BalanceManagerBase::btc();
        let eth = BalanceManagerBase::eth();
        let bnb = BalanceManagerBase::bnb();

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            hashmap![
                btc => dec!(2),
                eth => dec!(0.5),
                bnb => dec!(0.1)
            ],
        );

        let amount = dec!(5);
        let client_order_id = ClientOrderId::unique_id();

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            amount,
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .approve_reservation(reservation_id, &client_order_id, amount);

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            hashmap![
                btc => dec!(2),
                eth => dec!(0.5),
                bnb => dec!(0.1)
            ],
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(2))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                eth
            ),
            Some(dec!(0.5))
        );
        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                bnb
            ),
            Some(dec!(0.1))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_balance_partially_approved_reservations_are_not_counted_after_balance_update()
    {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let btc = BalanceManagerBase::btc();
        let eth = BalanceManagerBase::eth();
        let bnb = BalanceManagerBase::bnb();

        let mut balance_map = hashmap![
            btc => dec!(2),
            eth => dec!(0.5),
            bnb => dec!(0.1)
        ];

        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map.clone(),
        );

        let amount = dec!(5);
        let price = dec!(0.2);
        let approved_amount = amount / dec!(2);
        let client_order_id = ClientOrderId::unique_id();

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            amount,
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &client_order_id,
            approved_amount,
        );

        balance_map.insert(btc, dec!(1.5));
        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                eth
            ),
            Some(dec!(0.5))
        );
        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                bnb
            ),
            Some(dec!(0.1))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_filled_last_fill_by_default_buy() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(2), dec!(0.5), dec!(0.2)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0.5) + dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.2) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_filled_last_fill_by_default_sell() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(7), dec!(11), dec!(0.2)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(7) + price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(11) - dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.2) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_filled_specific_fill_buy() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(2), dec!(0.5), dec!(0.25)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object.balance_manager().order_was_filled_with_fill(
            configuration_descriptor,
            &order,
            order.fills.fills.first().expect("in test"),
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0.5) + dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.25) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_filled_specific_fill_sell() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(7), dec!(11), dec!(0.2)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object.balance_manager().order_was_filled_with_fill(
            configuration_descriptor,
            &order,
            order.fills.fills.first().expect("in test"),
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(7) + price * dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(11) - dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.2) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_finished_buy() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(2), dec!(0.5), dec!(0.2)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.fills.filled_amount = dec!(6);

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5) - price * dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0.5) + dec!(5) + dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.2) - dec!(0.1) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_finished_sell() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(7), dec!(11), dec!(0.2)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.fills.filled_amount = dec!(6);

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(7) + price * dec!(5) + price * dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(11) - dec!(5) - dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.2) - dec!(0.1) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn order_was_finished_buy_sell() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::btc(),
                BalanceManagerBase::eth(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(7), dec!(11), dec!(0.4)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.fills.filled_amount = dec!(6);

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(5),
            dec!(2.5),
        ));
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(2.5),
        ));
        order.fills.filled_amount = dec!(6);

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(7)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(11)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(0.4) - dec!(0.1) - dec!(0.1) - dec!(0.1) - dec!(0.1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_order_creating() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(2), dec!(1)],
        );

        let price = dec!(0.2);
        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.fills.filled_amount = order.amount() / dec!(2);
        order.set_status(OrderStatus::Creating, Utc::now());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order.header.side,
            price,
            order.amount(),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order.clone())));

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            Some(vec![order_ref]),
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(2)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_order_got_status_created_but_its_reservation_is_not_approved() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(11), dec!(1)],
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        order.set_status(OrderStatus::Created, Utc::now());
        let price = dec!(1.5) * order.price();

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order.header.side,
            price,
            order.amount(),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order.clone())));

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            Some(vec![order_ref]),
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(11) - price * order.amount()
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        test_object
            .balance_manager_base
            .get_balance_by_another_balance_manager_and_currency_code(
                &cloned_balance_manager.lock(),
                BalanceManagerBase::btc(),
                price,
            )
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(11)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_order_created() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(2), dec!(1)],
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        order.fills.filled_amount = order.amount() / dec!(2);
        order.set_status(OrderStatus::Created, Utc::now());
        let price = dec!(0.2);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order.header.side,
            price,
            order.amount(),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());
        test_object.balance_manager().approve_reservation(
            order.header.reservation_id.expect("in test"),
            &order.header.client_order_id,
            order.amount(),
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order.clone())));

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            Some(vec![order_ref]),
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        test_object
            .balance_manager_base
            .get_balance_by_another_balance_manager_and_currency_code(
                &cloned_balance_manager.lock(),
                BalanceManagerBase::btc(),
                price,
            )
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(2) - price * dec!(5) + price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_exists_unapproved_reservations() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(2), dec!(1)],
        );

        let price = dec!(0.2);
        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order.header.side,
            price,
            order.amount(),
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            None,
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        test_object
            .balance_manager_base
            .get_balance_by_another_balance_manager_and_currency_code(
                &cloned_balance_manager.lock(),
                BalanceManagerBase::btc(),
                price,
            )
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(2)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_exists_approved_reservation() {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(2), dec!(1)],
        );

        let price = dec!(0.2);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(5),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        test_object.balance_manager().approve_reservation(
            order.header.reservation_id.expect("in test"),
            &order.header.client_order_id,
            order.amount(),
        );

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            None,
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(2) - price * dec!(5)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_exists_partially_approved_reservation_1_approved_part_out_of_1_and_no_created_orders(
    ) {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(11), dec!(1)],
        );

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        let price = dec!(1.5) * order.price();
        let reservation_amount = dec!(3) * order.amount();

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order.header.side,
            price,
            reservation_amount,
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            None,
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(11) - price * reservation_amount
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(11) - price * order.amount()
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_exists_partially_approved_reservation_2_approved_part_out_of_2_and_no_created_orders(
    ) {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(11), dec!(1)],
        );

        let order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        let mut order_2 = test_object.balance_manager_base.create_order_by_amount(
            OrderSide::Buy,
            dec!(1.3) * order_1.amount(),
            ReservationId::generate(),
        );
        order_2.props.raw_price = Some(dec!(1.2) * order_2.price());

        let price = dec!(1.5) * order_1.price();
        let reservation_amount = dec!(3) * order_1.amount();

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order_1.header.side,
            price,
            reservation_amount,
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order_1.header.client_order_id,
            order_1.amount(),
        );

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order_2.header.client_order_id,
            order_2.amount(),
        );

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            None,
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(11) - price * reservation_amount
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(11) - price * (order_1.amount() + order_2.amount())
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_exists_partially_approved_reservation_1_approved_part_out_of_2_and_1_created_orders(
    ) {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(11), dec!(1)],
        );

        let order_1_amount = dec!(5);
        let order_2_amount = dec!(1.3) * order_1_amount;
        let price = dec!(1.5) * dec!(0.2);
        let reservation_amount = order_1_amount + order_2_amount;

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            reservation_amount,
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order_1 = test_object.balance_manager_base.create_order_by_amount(
            OrderSide::Buy,
            order_1_amount,
            reservation_id,
        );
        order_1.set_status(OrderStatus::Created, Utc::now());

        let mut order_2 = test_object.balance_manager_base.create_order_by_amount(
            OrderSide::Buy,
            order_2_amount,
            reservation_id,
        );
        order_2.props.raw_price = Some(dec!(1.2) * order_2.price());

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order_1.header.client_order_id,
            order_1.amount(),
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order_1.clone())));

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            Some(vec![order_ref]),
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(11) - price * reservation_amount
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(11)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_exists_partially_approved_reservation_2_approved_part_out_of_3_and_no_2_created_orders(
    ) {
        init_logger();
        let mut test_object = create_test_obj_with_multiple_currencies(
            vec![
                BalanceManagerBase::eth(),
                BalanceManagerBase::btc(),
                BalanceManagerBase::bnb(),
            ],
            vec![dec!(0), dec!(11), dec!(1)],
        );
        let order_1_amount = dec!(5);
        let order_2_amount = dec!(1.3) * order_1_amount;
        let order_3_amount = dec!(1.2) * order_1_amount;
        let price = dec!(1.5) * dec!(0.2);
        let reservation_amount = order_1_amount + order_2_amount + order_3_amount;

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            reservation_amount,
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order_1.set_status(OrderStatus::Created, Utc::now());

        let mut order_2 = test_object.balance_manager_base.create_order_by_amount(
            OrderSide::Buy,
            dec!(1.3) * order_1.amount(),
            reservation_id,
        );
        order_2.props.raw_price = Some(dec!(1.2) * order_2.price());
        order_2.set_status(OrderStatus::Creating, Utc::now());

        let mut order_3 = test_object.balance_manager_base.create_order_by_amount(
            OrderSide::Buy,
            dec!(1.1) * order_1.amount(),
            reservation_id,
        );
        order_3.props.raw_price = Some(dec!(1.1) * order_3.price());
        order_3.set_status(OrderStatus::Created, Utc::now());

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order_1.header.client_order_id,
            order_1.amount(),
        );

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order_3.header.client_order_id,
            order_3.amount(),
        );

        let order_pool = OrdersPool::new();
        let order_ref_1 = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order_1.clone())));
        let order_ref_2 = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order_2.clone())));
        let order_ref_3 = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order_3.clone())));

        let cloned_balance_manager = BalanceManager::clone_and_subtract_not_approved_data(
            test_object
                .balance_manager_base
                .balance_manager
                .as_ref()
                .expect("in test")
                .clone(),
            Some(vec![order_ref_1, order_ref_2, order_ref_3]),
        )
        .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price)
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(11) - price * reservation_amount
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::bnb(), price)
                .expect("in test"),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            dec!(0)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::btc(),
                    price
                )
                .expect("in test"),
            dec!(11)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::bnb(),
                    price
                )
                .expect("in test"),
            dec!(1)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unmatched_reserved_amount_and_order_amount_sum_sell() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(1000));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(99),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(33))
            .expect("in test");
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(33))
            .expect("in test");
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(34))
            .expect("in test");

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1000))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unmatched_reserved_amount_and_order_amount_sum_buy() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1000));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(99),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(33))
            .expect("in test");
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(33))
            .expect("in test");
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(34))
            .expect("in test");

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(1000))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_reservation_limits_enough_and_not_enough() {
        init_logger();
        let test_object = create_test_obj_by_currency_code_with_limit(
            BalanceManagerBase::btc(),
            dec!(1),
            Some(dec!(2)),
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(2),
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(2) * dec!(0.2))
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(2) * dec!(0.2) - dec!(2) * dec!(0.2))
        );

        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_fill_and_reservation_limits_enough_and_not_enough() {
        init_logger();
        let limit = dec!(2);
        let start_amount = dec!(1);
        let mut test_object = create_test_obj_by_currency_code_with_limit(
            BalanceManagerBase::btc(),
            dec!(1),
            Some(limit),
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .amount_currency_code,
            BalanceManagerBase::eth()
        );
        let buy_price = dec!(0.2);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            buy_price,
            start_amount,
        );

        let balance_before_reservations = limit * buy_price;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(balance_before_reservations)
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let reservation_amount = buy_price;
        let balance_after_reservation = balance_before_reservations - reservation_amount;

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(balance_after_reservation)
        );

        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        let fill_price = buy_price * dec!(0.5);
        let fill_amount = start_amount;
        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            fill_price,
            fill_amount,
            fill_price,
        ));
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        let position_by_fill_amount = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");
        assert_eq!(
            position_by_fill_amount
                .get(
                    test_object.balance_manager_base.exchange_account_id_1,
                    test_object.balance_manager_base.symbol().currency_pair(),
                )
                .expect("in test"),
            -fill_amount
        );

        let reservation_amount = reserve_parameters.amount;

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                configuration_descriptor,
                exchange_account_id,
                test_object.balance_manager_base.symbol(),
                OrderSide::Buy,
                fill_price
            ),
            Some(
                (dec!(0.7) / fill_price)
                    .min((limit - (reservation_amount + fill_amount)) * fill_price)
            )
        );

        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn restore_state_ctor() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0));
        let (_, exchanges_by_id) = BalanceManagerOrdinal::create_balance_manager_ctor_parameters();

        let currency_pair_to_symbol_converter = CurrencyPairToSymbolConverter::new(exchanges_by_id);

        let balance_manager = BalanceManager::new(currency_pair_to_symbol_converter.clone(), None);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let mut balance_map: HashMap<CurrencyCode, Amount> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(1));

        BalanceManagerBase::update_balance(
            &mut *balance_manager.lock(),
            exchange_account_id,
            balance_map,
        );

        let price = dec!(1);

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(0.08),
        );

        let reservation_id_1 = balance_manager
            .lock()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(0.1),
        );
        let reservation_id_2 = balance_manager
            .lock()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");

        let original_balances = balance_manager.lock().get_balances();

        test_object
            .balance_manager_base
            .set_balance_manager(BalanceManager::new(currency_pair_to_symbol_converter, None));

        test_object
            .balance_manager()
            .restore_balance_state_with_reservations_handling(&original_balances)
            .expect("in test");

        assert!(!test_object
            .balance_manager()
            .balance_was_received(exchange_account_id));

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price),
            None
        );
        let balance_manager = test_object.balance_manager();
        assert!(balance_manager.get_reservation(reservation_id_1).is_none());
        assert!(balance_manager.get_reservation(reservation_id_2).is_none());

        let all_balances = balance_manager.get_balances();
        assert_eq!(
            all_balances
                .virtual_diff_balances
                .expect("in test")
                .get_by_balance_request(
                    &test_object
                        .balance_manager_base
                        .create_balance_request(BalanceManagerBase::btc())
                )
                .expect("in test"),
            dec!(0)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn update_exchange_balance_should_ignore_approved_reservations_for_canceled_orders() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(10));

        let price = dec!(1);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            dec!(1),
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(9)
        );

        order.set_status(OrderStatus::Canceled, Utc::now());

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);

        let mut balance_map: HashMap<CurrencyCode, Amount> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(10));
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(9)
        );
        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price)
                .expect("in test"),
            dec!(10)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_should_reduce_not_approved_amount() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(9),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(reservation.amount, dec!(9));
        assert_eq!(reservation.unreserved_amount, dec!(9));
        assert_eq!(reservation.not_approved_amount, dec!(9));

        balance_manager
            .unreserve(reservation_id, dec!(6))
            .expect("in test");

        let reservation = balance_manager.get_reservation_expected(reservation_id);

        assert_eq!(reservation.unreserved_amount, dec!(3));
        assert_eq!(reservation.not_approved_amount, dec!(3));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_should_reduce_not_approved_amount_approved_order_single_unreserve() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(9),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.set_status(OrderStatus::Created, Utc::now());
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(9));
        assert_eq!(reservation.not_approved_amount, dec!(4));

        balance_manager
            .unreserve_by_client_order_id(
                reservation_id,
                order.header.client_order_id.clone(),
                order.amount(),
            )
            .expect("in test");

        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(4));
        assert_eq!(reservation.not_approved_amount, dec!(4));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_should_reduce_not_approved_amount_approved_order_unreserve_twice_by_half(
    ) {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(9),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.set_status(OrderStatus::Created, Utc::now());
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(9));
        assert_eq!(reservation.not_approved_amount, dec!(4));

        balance_manager
            .unreserve_by_client_order_id(
                reservation_id,
                order.header.client_order_id.clone(),
                order.amount() / dec!(2),
            )
            .expect("in test");

        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(6.5));
        assert_eq!(reservation.not_approved_amount, dec!(4));

        balance_manager
            .unreserve_by_client_order_id(
                reservation_id,
                order.header.client_order_id.clone(),
                order.amount() / dec!(2),
            )
            .expect("in test");

        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(4));
        assert_eq!(reservation.not_approved_amount, dec!(4));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "in test")]
    pub async fn unreserve_should_reduce_not_approved_amount_approved_order_unreserve_more_than_we_have(
    ) {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(9),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.set_status(OrderStatus::Created, Utc::now());
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(9));
        assert_eq!(reservation.not_approved_amount, dec!(4));

        balance_manager
            .unreserve_by_client_order_id(
                reservation_id,
                order.header.client_order_id.clone(),
                order.amount() + dec!(1),
            )
            .expect("in test");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn unreserve_should_reduce_not_approved_amount_not_approved_order_single_unreserve() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(9),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.set_status(OrderStatus::Created, Utc::now());
        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(9));
        assert_eq!(reservation.not_approved_amount, dec!(9));

        balance_manager
            .unreserve_by_client_order_id(
                reservation_id,
                order.header.client_order_id.clone(),
                dec!(5),
            )
            .expect("in test");

        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(4));
        assert_eq!(reservation.not_approved_amount, dec!(4));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[should_panic(expected = "in test")]
    pub async fn unreserve_should_reduce_not_approved_amount_not_approved_order_unreserve_more_than_we_have(
    ) {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            dec!(0.2),
            dec!(9),
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.set_status(OrderStatus::Created, Utc::now());
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        let mut balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id);
        assert_eq!(reservation.unreserved_amount, dec!(9));
        assert_eq!(reservation.not_approved_amount, dec!(4));

        balance_manager
            .unreserve(reservation_id, dec!(5))
            .expect("in test");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore] // TODO: Should be fixed by https://github.com/CryptoDreamTeam/CryptoDreamTraderSharp/issues/802
    pub async fn order_was_finished_should_unreserve_related_reservations() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(10));

        let price = dec!(1);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            dec!(1),
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(9))
        );

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(1),
            dec!(1),
        ));
        order.set_status(OrderStatus::Completed, Utc::now());

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(8))
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price),
            Some(dec!(1))
        );

        let mut balance_map: HashMap<CurrencyCode, Amount> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(9));
        balance_map.insert(BalanceManagerBase::eth(), dec!(1));
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(8))
        );

        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(9))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore] // TODO: Should be fixed by https://github.com/CryptoDreamTeam/CryptoDreamTraderSharp/issues/802
    pub async fn order_was_filled_should_unreserve_related_reservations() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let price = dec!(1);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            dec!(1),
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(9))
        );

        order.add_fill(BalanceManagerOrdinal::create_order_fill(
            price,
            dec!(0.5),
            dec!(1),
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        let mut balance_map: HashMap<CurrencyCode, Amount> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(9));
        balance_map.insert(BalanceManagerBase::eth(), dec!(0.5));
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        BalanceManagerBase::update_balance(
            &mut *test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(9))
        );

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(0.5))
            .expect("in test");
        order.set_status(OrderStatus::Canceled, Utc::now());
        test_object
            .balance_manager()
            .unreserve_rest(reservation_id)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), price),
            Some(dec!(9.5))
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price),
            Some(dec!(0.5))
        );
    }

    // Testing case when we reach limit for Sell and trying to create order for reaching limit for Buy then again for Sell
    // begin amount base - 100, quote - 100, limit 10
    // Steps:
    //  1) Sell 10(reach limit for sells)
    //  2) Buy 20(reach limit for purchases)
    //  3) Sell 20(reach limit for sells)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn try_reserve_with_limit_for_borderline_case() {
        // preparing
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(100), dec!(100));

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let trade_place = MarketAccountId::new(
            exchange_account_id,
            test_object.balance_manager_base.symbol().currency_pair(),
        );

        assert!(test_object
            .balance_manager()
            .get_last_position_change_before_period(&trade_place, test_object.now)
            .is_none());

        let price = dec!(0.2);
        let limit = dec!(10);

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;

        test_object.balance_manager().set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            test_object.balance_manager_base.symbol(),
            limit,
        );

        // 1
        let reserve_parameters_sell = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(10),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_sell, &mut None)
            .expect("should reserve full amount");

        let mut sell = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        sell.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(10),
            dec!(2.5),
            test_object.now,
        ));

        order_was_filled(&mut test_object, &mut sell);
        check_position(&test_object, dec!(-10));
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(20))
            .expect("in test");

        // 2
        let reserve_parameters_buy = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(20),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_buy, &mut None)
            .expect("should reserve from one side to another");

        let mut buy = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        buy.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(20),
            dec!(2.5),
            test_object.now,
        ));

        order_was_filled(&mut test_object, &mut buy);
        check_position(&test_object, dec!(10));
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(20))
            .expect("in test");

        // 3
        let mut sell = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        sell.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(20),
            dec!(2.5),
            test_object.now,
        ));

        let reserve_parameters_sell = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(20),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_sell, &mut None)
            .expect("should reserve full amount");

        order_was_filled(&mut test_object, &mut sell);
        check_position(&test_object, dec!(-10));
        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(20))
            .expect("in test");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_last_position_change_before_period_base_cases() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(10), dec!(0));

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let market_account_id = MarketAccountId::new(
            exchange_account_id,
            test_object.balance_manager_base.symbol().currency_pair(),
        );

        assert!(test_object
            .balance_manager()
            .get_last_position_change_before_period(&market_account_id, test_object.now)
            .is_none());

        let price = dec!(0.2);

        let mut sell_5 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());
        sell_5.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(5),
            dec!(2.5),
            test_object.now,
        ));

        let mut buy_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        buy_1.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(1),
            dec!(2.5),
            test_object.now,
        ));

        let mut buy_2 = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        buy_2.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(2),
            dec!(2.5),
            test_object.now,
        ));

        let mut buy_4 = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        buy_4.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(4),
            dec!(2.5),
            test_object.now,
        ));

        let mut buy_0 = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        buy_0.add_fill(BalanceManagerOrdinal::create_order_fill_with_time(
            price,
            dec!(0),
            dec!(2.5),
            test_object.now,
        ));

        let order_fill_id_1 = order_was_filled(&mut test_object, &mut sell_5);
        test_object.check_time(0);
        check_position(&test_object, dec!(-5));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(&market_account_id, test_object.now)
                .expect("in test"),
            PositionChange::new(order_fill_id_1.clone(), test_object.now, dec!(1))
        );
        test_object.timer_add_second();
        let _order_fill_id_2 = order_was_filled(&mut test_object, &mut buy_4);
        test_object.check_time(1);
        check_position(&test_object, dec!(-1));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(&market_account_id, test_object.now)
                .expect("in test"),
            PositionChange::new(order_fill_id_1, test_object.now, dec!(1))
        );

        test_object.timer_add_second();
        let order_fill_id_3 = order_was_filled(&mut test_object, &mut buy_4);
        test_object.check_time(2);
        check_position(&test_object, dec!(3));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(
                    &market_account_id,
                    test_object.now
                        + chrono::Duration::from_std(Duration::from_secs(2)).expect("in test")
                )
                .expect("in test"),
            PositionChange::new(
                order_fill_id_3.clone(),
                test_object.now
                    + chrono::Duration::from_std(Duration::from_secs(2)).expect("in test"),
                dec!(3) / dec!(4)
            )
        );

        test_object.timer_add_second();
        let _order_fill_id_4 = order_was_filled(&mut test_object, &mut buy_4);
        test_object.check_time(3);
        check_position(&test_object, dec!(7));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(
                    &market_account_id,
                    test_object.now
                        + chrono::Duration::from_std(Duration::from_secs(3)).expect("in test")
                )
                .expect("in test"),
            PositionChange::new(
                order_fill_id_3.clone(),
                test_object.now
                    + chrono::Duration::from_std(Duration::from_secs(2)).expect("in test"),
                dec!(3) / dec!(4)
            )
        );

        test_object.timer_add_second();
        let _order_fill_id_5 = order_was_filled(&mut test_object, &mut sell_5);
        test_object.check_time(4);
        check_position(&test_object, dec!(2));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(
                    &market_account_id,
                    test_object.now
                        + chrono::Duration::from_std(Duration::from_secs(4)).expect("in test")
                )
                .expect("in test"),
            PositionChange::new(
                order_fill_id_3,
                test_object.now
                    + chrono::Duration::from_std(Duration::from_secs(2)).expect("in test"),
                dec!(3) / dec!(4)
            )
        );

        test_object.timer_add_second();
        let order_fill_id_6 = order_was_filled(&mut test_object, &mut sell_5);
        test_object.check_time(5);
        check_position(&test_object, dec!(-3));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(
                    &market_account_id,
                    test_object.now
                        + chrono::Duration::from_std(Duration::from_secs(5)).expect("in test")
                )
                .expect("in test"),
            PositionChange::new(
                order_fill_id_6,
                test_object.now
                    + chrono::Duration::from_std(Duration::from_secs(5)).expect("in test"),
                dec!(3) / dec!(5)
            )
        );

        test_object.timer_add_second();
        let _order_fill_id_7 = order_was_filled(&mut test_object, &mut buy_1);
        let order_fill_id_8 = order_was_filled(&mut test_object, &mut buy_2);

        test_object.check_time(6);
        check_position(&test_object, dec!(0));
        assert_eq!(
            test_object
                .balance_manager()
                .get_last_position_change_before_period(
                    &market_account_id,
                    test_object.now
                        + chrono::Duration::from_std(Duration::from_secs(6)).expect("in test")
                )
                .expect("in test"),
            PositionChange::new(
                order_fill_id_8,
                test_object.now
                    + chrono::Duration::from_std(Duration::from_secs(6)).expect("in test"),
                dec!(0)
            )
        );
    }

    fn order_was_filled(
        test_object: &mut BalanceManagerOrdinal,
        order: &mut OrderSnapshot,
    ) -> ClientOrderFillId {
        let order_fill_id = ClientOrderFillId::unique_id();
        order
            .fills
            .fills
            .first_mut()
            .expect("in test")
            .set_client_order_fill_id(order_fill_id.clone());

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        let order_fill = order.fills.fills.first().expect("in test");
        test_object.balance_manager().order_was_filled_with_fill(
            configuration_descriptor,
            order,
            order_fill,
        );

        order_fill_id
    }

    fn check_position(test_object: &BalanceManagerOrdinal, position: Decimal) {
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        let amount_position = test_object.balance_manager().get_position(
            exchange_account_id,
            currency_pair,
            OrderSide::Buy,
        );

        assert_eq!(position, amount_position);
    }
}
