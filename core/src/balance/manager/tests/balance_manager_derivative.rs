use chrono::Utc;
use mmb_utils::hashmap;
use parking_lot::Mutex;
use parking_lot::MutexGuard;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use mmb_domain::exchanges::symbol::{Precision, Symbol};
use mmb_domain::market::ExchangeAccountId;
use mmb_domain::order::fill::{OrderFill, OrderFillType};
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_domain::order::snapshot::{OrderFillRole, OrderSide};
use std::collections::HashMap;
use std::sync::Arc;

use crate::balance::manager::tests::balance_manager_base::BalanceManagerBase;
use crate::{
    balance::manager::balance_manager::BalanceManager,
    exchanges::general::{
        currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter, exchange::Exchange,
        test_helper::get_test_exchange_with_symbol_and_id,
    },
};

pub struct BalanceManagerDerivative {
    balance_manager_base: BalanceManagerBase,
    exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
}

impl BalanceManagerDerivative {
    pub fn price() -> Price {
        dec!(0.2)
    }
    pub fn reversed_price_x_multiplier() -> Decimal {
        BalanceManagerDerivative::price() * BalanceManagerDerivative::reversed_amount_multiplier()
    }
    pub fn amount() -> Amount {
        dec!(1.9)
    }
    pub fn amount_reversed() -> Amount {
        dec!(1.9) / dec!(0.2)
    }
    pub fn reversed_amount_multiplier() -> Amount {
        dec!(0.001)
    }
    pub fn leverage() -> Decimal {
        dec!(7)
    }
    fn position() -> Decimal {
        dec!(1)
    }

    #[allow(clippy::type_complexity)]
    fn create_balance_manager(
        is_reversed: bool,
    ) -> (
        Arc<Symbol>,
        Arc<Mutex<BalanceManager>>,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let (symbol, exchanges_by_id) =
            BalanceManagerDerivative::create_balance_manager_ctor_parameters(is_reversed);
        let currency_pair_to_symbol_converter =
            CurrencyPairToSymbolConverter::new(exchanges_by_id.clone());

        let balance_manager = BalanceManager::new(currency_pair_to_symbol_converter, None);
        (symbol, balance_manager, exchanges_by_id)
    }

    fn create_balance_manager_ctor_parameters(
        is_reversed: bool,
    ) -> (Arc<Symbol>, HashMap<ExchangeAccountId, Arc<Exchange>>) {
        let base = BalanceManagerBase::eth();
        let quote = BalanceManagerBase::btc();

        let (balance, amount) = if is_reversed {
            (BalanceManagerBase::btc(), BalanceManagerBase::eth())
        } else {
            (BalanceManagerBase::eth(), BalanceManagerBase::btc())
        };

        let mut symbol = Symbol::new(
            true,
            base.as_str().into(),
            base,
            quote.as_str().into(),
            quote,
            None,
            None,
            None,
            None,
            None,
            amount,
            Some(balance),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(0.001) },
        );
        if is_reversed {
            symbol.amount_multiplier = dec!(0.001);
        }
        let symbol = Arc::from(symbol);
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

    fn new(is_reversed: bool) -> Self {
        let (symbol, balance_manager, exchanges_by_id) =
            BalanceManagerDerivative::create_balance_manager(is_reversed);
        let mut balance_manager_base = BalanceManagerBase::new();
        balance_manager_base.set_balance_manager(balance_manager);
        balance_manager_base.set_symbol(symbol);
        Self {
            balance_manager_base,
            exchanges_by_id,
        }
    }

    fn create_order_fill(
        price: Price,
        amount: Amount,
        cost: Decimal,
        commission_amount: Decimal,
        is_reversed: bool,
    ) -> OrderFill {
        let commission_currency_code = if is_reversed {
            BalanceManagerBase::btc()
        } else {
            BalanceManagerBase::eth()
        };
        OrderFill::new(
            Uuid::new_v4(),
            None,
            Utc::now(),
            OrderFillType::UserTrade,
            None,
            price,
            amount,
            cost,
            OrderFillRole::Taker,
            commission_currency_code,
            commission_amount,
            dec!(0),
            BalanceManagerBase::btc(),
            dec!(0),
            dec!(0),
            false,
            None,
            None,
        )
    }
}

impl BalanceManagerDerivative {
    pub fn balance_manager(&self) -> MutexGuard<BalanceManager> {
        self.balance_manager_base.balance_manager()
    }

    pub fn fill_order(
        &mut self,
        side: OrderSide,
        price: Option<Price>,
        amount: Option<Amount>,
        is_reversed: bool,
    ) {
        let price = price.unwrap_or_else(BalanceManagerDerivative::price);

        let amount = match amount {
            Some(amount) => amount,
            None => {
                if is_reversed {
                    BalanceManagerDerivative::amount_reversed()
                } else {
                    BalanceManagerDerivative::amount()
                }
            }
        };

        let reserve_parameters = self
            .balance_manager_base
            .create_reserve_parameters(side, price, amount);

        let reservation_id = self
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = self.balance_manager_base.create_order(side, reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            price,
            dec!(0),
            is_reversed,
        ));
        let configuration_descriptor = self.balance_manager_base.configuration_descriptor;
        self.balance_manager()
            .order_was_filled(configuration_descriptor, &order);
        self.balance_manager()
            .unreserve(reservation_id, amount)
            .expect("in test");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::Utc;
    use mmb_domain::order::snapshot::{Amount, Price};
    use mmb_utils::hashmap;
    use mmb_utils::logger::init_logger;
    use parking_lot::RwLock;
    use rstest::rstest;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::balance::manager::balance_manager::BalanceManager;
    use crate::balance::manager::tests::balance_manager_base::BalanceManagerBase;
    use crate::explanation::Explanation;
    use crate::infrastructure::init_lifetime_manager;
    use mmb_domain::market::CurrencyCode;

    use mmb_domain::order::pool::OrdersPool;
    use mmb_domain::order::snapshot::{OrderSide, OrderStatus, ReservationId};

    use super::BalanceManagerDerivative;

    fn create_eth_btc_test_obj(
        btc_amount: Amount,
        eth_amount: Amount,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        let test_object = BalanceManagerDerivative::new(is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let balance_map: HashMap<CurrencyCode, Amount> = hashmap![
            BalanceManagerBase::btc() => btc_amount,
            BalanceManagerBase::eth() => eth_amount
        ];

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            balance_map,
        );
        test_object
    }

    fn create_test_obj_by_currency_code(
        currency_code: CurrencyCode,
        amount: Amount,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        create_test_obj_by_currency_code_with_limit(currency_code, amount, None, is_reversed)
    }

    fn create_test_obj_by_currency_code_with_limit(
        currency_code: CurrencyCode,
        amount: Amount,
        limit: Option<Amount>,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        create_test_obj_by_currency_code_and_symbol_currency_pair(
            currency_code,
            amount,
            limit,
            is_reversed,
            None,
        )
    }

    fn create_test_obj_by_currency_code_and_symbol_currency_pair(
        currency_code: CurrencyCode,
        amount: Amount,
        limit: Option<Amount>,
        is_reversed: bool,
        symbol_currency_pair_amount: Option<Amount>,
    ) -> BalanceManagerDerivative {
        init_lifetime_manager();

        let test_object = BalanceManagerDerivative::new(is_reversed);

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

        let balance_map = hashmap![currency_code => amount];
        if let Some(symbol_currency_pair_amount) = symbol_currency_pair_amount {
            let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
            BalanceManagerBase::update_balance_with_positions(
                &mut test_object.balance_manager(),
                exchange_account_id,
                balance_map,
                hashmap![symbol_currency_pair => symbol_currency_pair_amount],
            );
        } else {
            BalanceManagerBase::update_balance(
                &mut test_object.balance_manager(),
                exchange_account_id,
                balance_map,
            );
        }
        test_object
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_should_use_balance_currency() {
        init_logger();
        let test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), false);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            dec!(5),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(
                    BalanceManagerBase::eth(),
                    BalanceManagerDerivative::price()
                )
                .expect("in test"),
            (dec!(100) - dec!(5) / BalanceManagerDerivative::price()) * dec!(0.95)
        );

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(4),
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(
                    BalanceManagerBase::eth(),
                    BalanceManagerDerivative::price()
                )
                .expect("in test"),
            (dec!(100) - dec!(4) / BalanceManagerDerivative::price()) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_should_use_balance_currency_reversed() {
        init_logger();
        let test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), true);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            dec!(5),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(
                    BalanceManagerBase::btc(),
                    BalanceManagerDerivative::price()
                )
                .expect("in test"),
            (dec!(100) - dec!(5) * BalanceManagerDerivative::reversed_price_x_multiplier())
                * dec!(0.95)
        );

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(4),
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(
                    BalanceManagerBase::btc(),
                    BalanceManagerDerivative::price()
                )
                .expect("in test"),
            (dec!(100) - dec!(4) * BalanceManagerDerivative::reversed_price_x_multiplier())
                * dec!(0.95)
        );
    }

    // TODO: add log checking must contain an error
    #[rstest]
    #[case(OrderSide::Buy, true)]
    #[case(OrderSide::Sell, true)]
    #[case(OrderSide::Buy, false)]
    #[case(OrderSide::Sell, false)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn position_more_than_limit_should_log_error(
        #[case] order_side: OrderSide,
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), false);

        let limit = dec!(2);
        let fill_amount = dec!(3);

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        let symbol = test_object.balance_manager_base.symbol();

        test_object.balance_manager().set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            symbol,
            limit,
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(order_side, ReservationId::generate());
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            fill_amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));
        test_object
            .balance_manager()
            .order_was_finished(configuration_descriptor, &order);
    }

    #[rstest]
    #[case(OrderSide::Buy, dec!(1), None, true)]
    #[case(OrderSide::Sell, dec!(1),None, true)]
    #[case(OrderSide::Buy, dec!(1), Some(dec!(5)), true)]
    #[case(OrderSide::Sell, dec!(1),Some(dec!(5)), true)]
    #[case(OrderSide::Buy, dec!(1), None,false)]
    #[case(OrderSide::Sell, dec!(1),None, false)]
    #[case(OrderSide::Buy, dec!(1), Some(dec!(5)),false)]
    #[case(OrderSide::Sell, dec!(1),Some(dec!(5)), false)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fill_should_change_position(
        #[case] order_side: OrderSide,
        #[case] expected_position: Decimal,
        #[case] leverage: Option<Decimal>,
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        if let Some(leverage) = leverage {
            let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
            let symbol = test_object.balance_manager_base.symbol();
            test_object
                .exchanges_by_id
                .get_mut(&exchange_account_id)
                .expect("in test")
                .leverage_by_currency_pair
                .insert(symbol.currency_pair(), leverage);
        }

        let mut order = test_object
            .balance_manager_base
            .create_order(order_side, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object.balance_manager().get_position(
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol().currency_pair(),
                order_side,
            ),
            expected_position
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fill_buy_should_commission_should_be_deducted_from_balance() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), false);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            false,
        ));
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), dec!(0.1))
                .expect("in test"),
            (dec!(100) + dec!(0.00005)) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), dec!(0.1))
                .expect("in test"),
            (dec!(100) * dec!(0.1) - dec!(1) / dec!(0.1) / dec!(5) * dec!(0.1)
                + dec!(0.00005) * dec!(0.1))
                * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fill_buy_should_commission_should_be_deducted_from_balance_reversed() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), true);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            true,
        ));
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), dec!(0.1))
                .expect("in test"),
            (dec!(100) / dec!(0.1) + dec!(0.00005) / dec!(0.1)) * dec!(0.95)
        );

        let multiplier = BalanceManagerDerivative::reversed_amount_multiplier();
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), dec!(0.1))
                .expect("in test"),
            (dec!(100) - dec!(1) * dec!(0.1) / dec!(5) * multiplier + dec!(0.00005)) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fill_sell_should_commission_should_be_deducted_from_balance() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), dec!(0.1))
                .expect("in test"),
            (dec!(100) - dec!(1) / dec!(0.1) / dec!(5) + dec!(0.00005)) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), dec!(0.1))
                .expect("in test"),
            (dec!(100) * dec!(0.1) + dec!(0.00005) * dec!(0.1)) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fill_sell_should_commission_should_be_deducted_from_balance_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        let multiplier = BalanceManagerDerivative::reversed_amount_multiplier();
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), dec!(0.1))
                .expect("in test"),
            (dec!(100) / dec!(0.1) - dec!(1) / dec!(5) * multiplier + dec!(0.00005) / dec!(0.1))
                * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), dec!(0.1))
                .expect("in test"),
            (dec!(100) + dec!(0.00005)) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_the_same_direction_buy_should_be_not_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(1))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(100) * dec!(0.95)
        );

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Sell
            ),
            dec!(-1)
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.6) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_the_same_direction_buy_should_be_not_free_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            amount,
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, amount)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(1000) * dec!(0.95)
        );

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Sell
            ),
            -amount
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9996) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_the_same_direction_sell_should_be_not_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(1),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(1))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(10) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy
            ),
            dec!(-1)
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(96) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_the_same_direction_sell_should_be_not_free_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            amount,
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, amount)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(100) * dec!(0.95)
        );

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy
            ),
            -amount
        );

        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.996) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_opposite_direction_buy_sell_should_be_partially_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(1))
            .expect("in test");

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy
            ),
            dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(100) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(1.5),
        );
        let partially_free_reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.7) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(97) * dec!(0.95)
        );

        //the whole 1.5 is not free as we've taken the whole free position with the previous reservation
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.4) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(94) * dec!(0.95)
        );

        //free amount from position is available again
        test_object
            .balance_manager()
            .unreserve(partially_free_reservation_id, dec!(1.5))
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.5) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(97) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_opposite_direction_buy_sell_should_be_partially_free_reversed(
    ) {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            amount,
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, amount)
            .expect("in test");

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy
            ),
            amount
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(1000) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            amount * dec!(1.5),
        );

        //1 out of 1.5 is free
        let partially_free_reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9997) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.997) * dec!(0.95)
        );

        //the whole 1.5 is not free as we've taken the whole free position with the previous reservation
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9994) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.994) * dec!(0.95)
        );

        //free amount from position is available again
        test_object
            .balance_manager()
            .unreserve(partially_free_reservation_id, amount * dec!(1.5))
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9995) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.997) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_opposite_direction_sell_buy_should_be_partially_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(1),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.8) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, dec!(1))
            .expect("in test");

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy
            ),
            dec!(-1)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(10) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(98) * dec!(0.95)
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1.5),
        );

        //1 out of 1.5 is free
        let partially_free_reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.7) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(97) * dec!(0.95)
        );

        //the whole 1.5 is not free as we've taken the whole free position with the previous reservation
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.4) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(94) * dec!(0.95)
        );

        //free amount from position is available again
        test_object
            .balance_manager()
            .unreserve(partially_free_reservation_id, dec!(1.5))
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(9.7) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(95) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_after_fill_in_opposite_direction_sell_buy_should_be_partially_free_reversed(
    ) {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            amount,
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9998) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::generate());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        test_object
            .balance_manager()
            .unreserve(reservation_id, amount)
            .expect("in test");

        assert_eq!(
            test_object.balance_manager().get_position(
                exchange_account_id,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy
            ),
            -amount
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.998) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(100) * dec!(0.95)
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            amount * dec!(1.5),
        );

        //1 out of 1.5 is free
        let partially_reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(OrderSide::Buy, price, amount * dec!(1.5));

        let partially_free_reservation_id = test_object
            .balance_manager()
            .try_reserve(&partially_reserve_parameters, &mut None)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9997) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.997) * dec!(0.95)
        );

        //the whole 1.5 is not free as we've taken the whole free position with the previous reservation
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9994) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.994) * dec!(0.95)
        );

        //free amount from position is available again
        test_object
            .balance_manager()
            .unreserve(partially_free_reservation_id, amount * dec!(1.5))
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price)
                .expect("in test"),
            dec!(999.995) * dec!(0.95)
        );
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price)
                .expect("in test"),
            dec!(99.9997) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_order_got_status_created_but_its_reservation_is_not_approved_possible_precision_error(
    ) {
        // This case may happen because parallel nature of handling orders

        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(10), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.2),
            dec!(5),
        );
        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, reservation_id);
        order.set_status(OrderStatus::Created, Utc::now());

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order.clone())));

        // ApproveReservation wait on lock after Clone started
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

        // TODO: add log checking
        // TestCorrelator.GetLogEventsFromCurrentContext().Should().NotContain(logEvent => logEvent.Level == LogEventLevel.Error || logEvent.Level == LogEventLevel.Fatal);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), order.price())
                .expect("in test"),
            (dec!(10) - order.amount() / order.price() / dec!(5)) * dec!(0.95)
        );

        //cloned BalancedManager should be without reservation
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    order.price()
                )
                .expect("in test"),
            dec!(10) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn clone_when_order_created() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(10), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

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

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.fills.filled_amount = order.amount() / dec!(2);
        order.set_status(OrderStatus::Created, Utc::now());

        test_object.balance_manager().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        let order_pool = OrdersPool::new();
        let order_ref = order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));

        // ApproveReservation wait on lock after Clone started
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
            (dec!(10) - price / dec!(0.2) * dec!(5)) * dec!(0.95)
        );

        //cloned BalancedManager should be without reservation
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager.lock(),
                    BalanceManagerBase::eth(),
                    price
                )
                .expect("in test"),
            (dec!(10) - price / dec!(0.2) * dec!(5) + price / dec!(0.2) * dec!(5)) * dec!(0.95)
        );
    }

    #[rstest]
    #[ignore] // Transfer
    #[case(dec!(25), dec!(0.2), dec!(3), dec!(0.5), dec!(2) ,dec!(2) )] // Optimistic case: price1 < price2
    #[ignore] // Transfer
    #[case(dec!(25), dec!(0.5), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )] // Pessimistic case: price1 > price2
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservation_different_price_success(
        #[case] src_balance: Amount,
        #[case] price_1: Price,
        #[case] amount_1: Amount,
        #[case] price_2: Price,
        #[case] amount_2: Amount,
        #[case] amount_to_transfer: Amount,
    ) {
        init_logger();
        let is_reversed = false;
        let test_object = create_eth_btc_test_obj(src_balance, src_balance, is_reversed);

        let side = OrderSide::Sell;

        let common_params =
            test_object
                .balance_manager_base
                .create_reserve_parameters(side, price_1, dec!(0));
        let initial_balance = test_object
            .balance_manager()
            .get_balance_by_reserve_parameters(&common_params)
            .expect("in test");

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(side, price_1, amount_1);
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let mut balance_manager = test_object.balance_manager();
        let reservation_1 = balance_manager
            .get_reservation_expected(reservation_id_1)
            .clone();
        let balance_1 =
            initial_balance - reservation_1.convert_in_reservation_currency(reservation_1.amount);
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&common_params),
            Some(balance_1)
        );

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(side, price_2, amount_2);
        let reservation_id_2 = balance_manager
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        let reservation_2 = balance_manager
            .get_reservation_expected(reservation_id_2)
            .clone();
        let balance_2 =
            balance_1 - reservation_2.convert_in_reservation_currency(reservation_2.amount);
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&common_params),
            Some(balance_2)
        );

        assert!(balance_manager.try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            amount_to_transfer,
            &None
        ));

        let add = reservation_1.convert_in_reservation_currency(amount_to_transfer);
        let sub = reservation_2.convert_in_reservation_currency(amount_to_transfer);
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&common_params),
            Some(balance_2 + add - sub)
        );
        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);

        assert_eq!(reservation.cost, dec!(3) - dec!(2));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);

        assert_eq!(reservation.cost, dec!(2) + dec!(2));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[rstest]
    #[ignore] // Transfer
    #[case(dec!(20), dec!(0.5), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )] // Pessimistic case: price1 > price2
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn transfer_reservation_different_price_failure(
        #[case] src_balance: Amount,
        #[case] price_1: Price,
        #[case] amount_1: Amount,
        #[case] price_2: Price,
        #[case] amount_2: Amount,
        #[case] amount_to_transfer: Amount,
    ) {
        init_logger();
        let is_reversed = false;
        let test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), src_balance, is_reversed);

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price_1,
            amount_1,
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        let balance_1 = src_balance - amount_1 / price_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_1)
        );

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price_2,
            amount_2,
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        let balance_2 = balance_1 - amount_2 / price_2;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
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
            Some(balance_2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore] // Transfer
    pub async fn transfer_reservations_amount_partial() {
        init_logger();
        let is_reversed = false;
        let test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(30), is_reversed);

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

        let expected_balance = dec!(5);
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(expected_balance)
        );

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
            Some(expected_balance)
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore] // Transfer
    pub async fn transfer_reservations_amount_partial_with_cost_diff_due_to_fill() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(25), is_reversed);

        let price = dec!(0.2);

        let reserve_parameters_1 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(3),
        );
        let reservation_id_1 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_1, &mut None)
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation_expected(reservation_id_1)
                .cost,
            dec!(3)
        );

        let buy_reservation_params = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            dec!(1),
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&buy_reservation_params),
            Some(dec!(25) * price - dec!(3))
        );

        let buy_reservation_id = test_object
            .balance_manager()
            .try_reserve(&buy_reservation_params, &mut None)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, buy_reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        assert_eq!(
            test_object.balance_manager().get_position(
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol().currency_pair(),
                OrderSide::Buy,
            ),
            dec!(1)
        );
        test_object
            .balance_manager()
            .unreserve(buy_reservation_id, order.amount())
            .expect("in test");

        let reserve_parameters_2 = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(1.9),
        );
        let reservation_id_2 = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters_2, &mut None)
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation_expected(reservation_id_1)
                .cost,
            dec!(0.9)
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0.5))
        );

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
            Some(dec!(0.5))
        );

        let balance_manager = test_object.balance_manager();
        let reservation = balance_manager.get_reservation_expected(reservation_id_1);
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.cost, dec!(3) - dec!(2));

        let reservation = balance_manager.get_reservation_expected(reservation_id_2);
        assert_eq!(reservation.amount, dec!(1.9) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(1.9) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(1.9) + dec!(2));
        assert_eq!(reservation.cost, dec!(0.9) + dec!(2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn update_exchange_balance_should_use_cost_for_balance_filter_when_no_free_cost() {
        init_logger();
        let is_reversed = false;
        let test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(25), is_reversed);

        let price = dec!(0.2);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(2),
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some((dec!(25) - dec!(2) / price) * dec!(0.95))
        );
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_exchange_balance(
                    exchange_account_id,
                    test_object.balance_manager_base.symbol(),
                    BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25)
        );

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap!(BalanceManagerBase::eth() => dec!(25)),
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price),
            Some((dec!(25) - dec!(2) / price) * dec!(0.95))
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_exchange_balance(
                    exchange_account_id,
                    test_object.balance_manager_base.symbol(),
                    BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25) - dec!(2) / price
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn update_exchange_balance_should_use_cost_for_balance_filter_when_partially_free_cost(
    ) {
        init_logger();
        let is_reversed = false;
        let test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(25),
            None,
            is_reversed,
            Some(dec!(1)),
        );

        let price = dec!(0.2);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            dec!(2),
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some((dec!(25) - (dec!(2) - dec!(1)) / price) * dec!(0.95))
        );
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_exchange_balance(
                    exchange_account_id,
                    test_object.balance_manager_base.symbol(),
                    BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25)
        );

        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth() => dec!(25)],
            hashmap![symbol_currency_pair => dec!(1)],
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), price),
            Some((dec!(25) - (dec!(2) - dec!(1)) / price) * dec!(0.95))
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_exchange_balance(
                    exchange_account_id,
                    test_object.balance_manager_base.symbol(),
                    BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25) - (dec!(2) - dec!(1)) / price
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_no_limit_buy_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        let original_balance = dec!(9);
        let position = dec!(1);

        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth() => original_balance],
            hashmap![symbol_currency_pair => position],
        );

        let mut buy_balance = original_balance * price;
        let mut sell_balance =
            original_balance + position / price / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some(sell_balance * dec!(0.95))
        );

        let fill_amount = dec!(0.3);
        test_object.fill_order(OrderSide::Buy, None, Some(fill_amount), is_reversed);

        buy_balance = original_balance * price - fill_amount / BalanceManagerDerivative::leverage();
        sell_balance = original_balance
            - fill_amount / price / BalanceManagerDerivative::leverage()
            + (position + fill_amount) / price / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some(sell_balance * dec!(0.95))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_no_limit_buy_enough_and_not_enough_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(7));

        let original_balance = dec!(9) / price;
        let position = dec!(1) / price;

        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::btc() => original_balance],
            hashmap![symbol_currency_pair=> position],
        );

        let mut buy_balance = original_balance;
        let mut sell_balance = original_balance / price
            + position / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some(sell_balance * dec!(0.95))
        );

        let fill_amount = dec!(0.3);
        test_object.fill_order(OrderSide::Buy, None, Some(fill_amount), is_reversed);

        buy_balance = original_balance
            - fill_amount * price / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier();
        sell_balance = original_balance / price
            + position / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager_base
                        .get_balance_by_trade_side(OrderSide::Sell, price)
                        .expect("in test"),
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(sell_balance * dec!(0.95))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_no_limit_sell_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        let original_balance = dec!(9);
        let position = dec!(1);

        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth() => original_balance],
            hashmap![symbol_currency_pair => position],
        );

        let mut buy_balance = original_balance * price;
        let mut sell_balance =
            original_balance + position / price / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some(sell_balance * dec!(0.95))
        );

        let fill_amount = dec!(0.3);
        test_object.fill_order(OrderSide::Sell, None, Some(fill_amount), is_reversed);

        buy_balance = original_balance * price + fill_amount / BalanceManagerDerivative::leverage();
        sell_balance = original_balance
            + fill_amount / price / BalanceManagerDerivative::leverage()
            + (position - fill_amount) / price / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some(sell_balance * dec!(0.95))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_no_limit_sell_enough_and_not_enough_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(7));

        let original_balance = dec!(9) / price;
        let position = dec!(1) / price;

        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::btc()=> original_balance],
            hashmap![symbol_currency_pair=> position],
        );

        let mut buy_balance = original_balance;
        let mut sell_balance = original_balance / price
            + position / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some(sell_balance * dec!(0.95))
        );

        let fill_amount = dec!(0.3);
        test_object.fill_order(OrderSide::Sell, None, Some(fill_amount), is_reversed);

        buy_balance = original_balance
            + fill_amount * price / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier();
        sell_balance = original_balance / price
            + fill_amount / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier()
            + (position - fill_amount) / BalanceManagerDerivative::leverage()
                * BalanceManagerDerivative::reversed_amount_multiplier();

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Buy, price),
            Some(buy_balance * dec!(0.95))
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager_base
                        .get_balance_by_trade_side(OrderSide::Sell, price)
                        .expect("in test"),
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(sell_balance * dec!(0.95))
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_limit_buy_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;

        let amount_limit = dec!(2);
        test_object.balance_manager().set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            symbol.clone(),
            amount_limit,
        );

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth()=> dec!(1000)],
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            BalanceManagerDerivative::amount(),
        );

        let balance_before_reservation = amount_limit / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(balance_before_reservation)
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let reserved_amount = reserve_parameters.amount;
        let balance_after_reservation =
            balance_before_reservation - reserved_amount / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(balance_after_reservation)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            BalanceManagerDerivative::amount(),
            price,
            dec!(0),
            is_reversed,
        ));

        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);
        test_object
            .balance_manager()
            .unreserve(reservation_id, reserved_amount)
            .expect("in test");

        let position_by_fill_amount = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");

        assert_eq!(
            position_by_fill_amount
                .get(exchange_account_id, symbol.currency_pair())
                .expect("in test"),
            BalanceManagerDerivative::amount()
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_side(
                    configuration_descriptor,
                    exchange_account_id,
                    symbol.clone(),
                    OrderSide::Buy,
                    price
                )
                .expect("in test"),
            balance_before_reservation
                - BalanceManagerDerivative::amount() / BalanceManagerDerivative::leverage()
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_side(
                            configuration_descriptor,
                            exchange_account_id,
                            symbol,
                            OrderSide::Sell,
                            price
                        )
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    (balance_before_reservation
                        + BalanceManagerDerivative::amount()
                            / BalanceManagerDerivative::leverage())
                        / price
                )
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_limit_buy_enough_and_not_enough_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;

        let price = BalanceManagerDerivative::price();
        let amount = BalanceManagerDerivative::amount_reversed();
        let amount_multiplier = BalanceManagerDerivative::reversed_amount_multiplier();
        let amount_limit = dec!(2);
        let adjusted_amount_limit = amount_limit / price / amount_multiplier;
        test_object.balance_manager().set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            symbol.clone(),
            adjusted_amount_limit,
        );

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::btc()=> dec!(1000)],
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            price,
            amount,
        );

        let balance_before_reservation = amount_limit / BalanceManagerDerivative::leverage();

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(balance_before_reservation)
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let reserved_amount = reserve_parameters.amount;
        let balance_after_reservation = balance_before_reservation
            - reserved_amount / BalanceManagerDerivative::leverage() * price * amount_multiplier;

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(balance_after_reservation)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            price,
            dec!(0),
            is_reversed,
        ));

        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);
        test_object
            .balance_manager()
            .unreserve(reservation_id, reserved_amount)
            .expect("in test");

        let position_by_fill_amount = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");

        assert_eq!(
            position_by_fill_amount
                .get(exchange_account_id, symbol.currency_pair())
                .expect("in test"),
            amount
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_side(
                    configuration_descriptor,
                    exchange_account_id,
                    symbol.clone(),
                    OrderSide::Buy,
                    price
                )
                .expect("in test"),
            balance_before_reservation
                - amount / BalanceManagerDerivative::leverage() * price * amount_multiplier
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_side(
                            configuration_descriptor,
                            exchange_account_id,
                            symbol,
                            OrderSide::Sell,
                            price
                        )
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    (balance_before_reservation
                        + amount / BalanceManagerDerivative::leverage()
                            * price
                            * amount_multiplier)
                        / price
                )
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_limit_sell_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;

        let amount_limit = dec!(2);
        test_object.balance_manager().set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            symbol.clone(),
            amount_limit,
        );

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth()=> dec!(1000)],
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            BalanceManagerDerivative::amount(),
        );

        let balance_before_reservation =
            amount_limit / BalanceManagerDerivative::leverage() / price;

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_reserve_parameters(&reserve_parameters)
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(balance_before_reservation)
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let reserved_amount = reserve_parameters.amount;
        let balance_after_reservation = balance_before_reservation
            - reserved_amount / BalanceManagerDerivative::leverage() / price;

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_reserve_parameters(&reserve_parameters)
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(balance_after_reservation)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            BalanceManagerDerivative::amount(),
            price,
            dec!(0),
            is_reversed,
        ));

        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);
        test_object
            .balance_manager()
            .unreserve(reservation_id, reserved_amount)
            .expect("in test");

        let position_by_fill_amount = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");

        assert_eq!(
            position_by_fill_amount
                .get(exchange_account_id, symbol.currency_pair())
                .expect("in test"),
            -BalanceManagerDerivative::amount()
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_side(
                    configuration_descriptor,
                    exchange_account_id,
                    symbol.clone(),
                    OrderSide::Buy,
                    price
                )
                .expect("in test"),
            (balance_before_reservation
                + BalanceManagerDerivative::amount()
                    / BalanceManagerDerivative::leverage()
                    / price)
                * price
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_side(
                            configuration_descriptor,
                            exchange_account_id,
                            symbol,
                            OrderSide::Sell,
                            price
                        )
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    balance_before_reservation
                        - BalanceManagerDerivative::amount()
                            / BalanceManagerDerivative::leverage()
                            / price
                )
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn fills_and_reservations_limit_sell_enough_and_not_enough_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0), is_reversed);

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;

        let price = BalanceManagerDerivative::price();
        let amount = BalanceManagerDerivative::amount_reversed();
        let amount_multiplier = BalanceManagerDerivative::reversed_amount_multiplier();
        let amount_limit = dec!(2);
        let adjusted_amount_limit = amount_limit / price / amount_multiplier;
        test_object.balance_manager().set_target_amount_limit(
            configuration_descriptor,
            exchange_account_id,
            symbol.clone(),
            adjusted_amount_limit,
        );

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::btc() => dec!(1000)],
        );

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            price,
            amount,
        );

        let balance_before_reservation =
            amount_limit / BalanceManagerDerivative::leverage() / price;

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_reserve_parameters(&reserve_parameters)
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(balance_before_reservation)
        );

        let reservation_id = test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .expect("in test");

        let reserved_amount = reserve_parameters.amount;
        let balance_after_reservation = balance_before_reservation
            - reserved_amount / BalanceManagerDerivative::leverage() * amount_multiplier;

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_reserve_parameters(&reserve_parameters)
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(balance_after_reservation)
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            price,
            dec!(0),
            is_reversed,
        ));

        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);
        test_object
            .balance_manager()
            .unreserve(reservation_id, reserved_amount)
            .expect("in test");

        let position_by_fill_amount = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");

        assert_eq!(
            position_by_fill_amount
                .get(exchange_account_id, symbol.currency_pair())
                .expect("in test"),
            -amount
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_side(
                    configuration_descriptor,
                    exchange_account_id,
                    symbol.clone(),
                    OrderSide::Buy,
                    price
                )
                .expect("in test"),
            (balance_before_reservation
                + amount / BalanceManagerDerivative::leverage() * amount_multiplier)
                * price
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    test_object
                        .balance_manager()
                        .get_balance_by_side(
                            configuration_descriptor,
                            exchange_account_id,
                            symbol,
                            OrderSide::Sell,
                            price
                        )
                        .expect("in test")
                ),
            test_object
                .balance_manager_base
                .symbol()
                .round_to_remove_amount_precision_error(
                    balance_before_reservation
                        - amount / BalanceManagerDerivative::leverage() * amount_multiplier
                )
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_no_limit_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(10),
            None,
            is_reversed,
            Some(BalanceManagerDerivative::position()),
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position() + dec!(1.9) * BalanceManagerDerivative::leverage(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position() + dec!(2) * BalanceManagerDerivative::leverage(),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(1.9) * BalanceManagerDerivative::leverage(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(2) * BalanceManagerDerivative::leverage(),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_no_limit_enough_and_not_enough_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::btc(),
            dec!(2),
            None,
            is_reversed,
            Some(BalanceManagerDerivative::position()),
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position()
                + dec!(1.9) / BalanceManagerDerivative::price()
                    * BalanceManagerDerivative::leverage()
                    / BalanceManagerDerivative::reversed_amount_multiplier(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position()
                + dec!(2) / BalanceManagerDerivative::price()
                    * BalanceManagerDerivative::leverage()
                    / BalanceManagerDerivative::reversed_amount_multiplier(),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(1.9) / BalanceManagerDerivative::price() * BalanceManagerDerivative::leverage()
                / BalanceManagerDerivative::reversed_amount_multiplier(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(2) / BalanceManagerDerivative::price() * BalanceManagerDerivative::leverage()
                / BalanceManagerDerivative::reversed_amount_multiplier(),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_limit_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(10),
            Some(dec!(2)),
            is_reversed,
            Some(BalanceManagerDerivative::position()),
        );

        let symbol = test_object.balance_manager_base.symbol();
        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position() + dec!(2),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position() + dec!(2) + dec!(0.0000000001),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(2) - BalanceManagerDerivative::position(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            dec!(2) + dec!(0.0000000001) - BalanceManagerDerivative::position(),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_limit_enough_and_not_enough_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::btc(),
            dec!(2),
            Some(
                dec!(2)
                    / BalanceManagerDerivative::price()
                    / BalanceManagerDerivative::reversed_amount_multiplier(),
            ),
            is_reversed,
            Some(BalanceManagerDerivative::position()),
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), BalanceManagerDerivative::leverage());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position()
                + dec!(2)
                    / BalanceManagerDerivative::price()
                    / BalanceManagerDerivative::reversed_amount_multiplier(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            BalanceManagerDerivative::price(),
            BalanceManagerDerivative::position()
                + dec!(2)
                    / BalanceManagerDerivative::price()
                    / BalanceManagerDerivative::reversed_amount_multiplier()
                + dec!(0.0000000001),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            -BalanceManagerDerivative::position()
                + dec!(2)
                    / BalanceManagerDerivative::price()
                    / BalanceManagerDerivative::reversed_amount_multiplier(),
        );
        assert!(test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Buy,
            BalanceManagerDerivative::price(),
            -BalanceManagerDerivative::position()
                + dec!(2)
                    / BalanceManagerDerivative::price()
                    / BalanceManagerDerivative::reversed_amount_multiplier()
                + dec!(0.0000000001),
        );
        assert!(!test_object
            .balance_manager()
            .can_reserve(&reserve_parameters, &mut None));
    }

    #[rstest]
    #[case(OrderSide::Sell, true, false)]
    #[case(OrderSide::Buy, false, false)]
    #[case(OrderSide::Sell, true, true)]
    #[case(OrderSide::Buy, false, true)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn can_reserve_when_out_of_limit_and_moving_to_the_limit(
        #[case] order_side: OrderSide,
        #[case] expected_can_reserve: bool,
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(1000),
            Some(dec!(450)),
            is_reversed,
            Some(dec!(610)),
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(3));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            order_side,
            dec!(9570),
            dec!(30),
        );
        assert_eq!(
            test_object
                .balance_manager()
                .can_reserve(&reserve_parameters, &mut None),
            expected_can_reserve
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_more_than_limit_long_position(
    ) {
        init_logger();
        let amount_limit = dec!(5);
        let is_reversed = false;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(10),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Buy, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(margin_buy, dec!(5) - dec!(1.9));

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(margin_sell, (dec!(5) + dec!(1.9)) / dec!(0.2) * dec!(0.2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_more_than_limit_long_position_reversed(
    ) {
        init_logger();
        let amount_limit = dec!(5) / BalanceManagerDerivative::price();
        let is_reversed = true;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::btc(),
            dec!(100),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Buy, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(
            margin_buy,
            amount_limit - BalanceManagerDerivative::amount_reversed()
        );

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(
            margin_sell,
            (amount_limit + BalanceManagerDerivative::amount_reversed()) / dec!(0.2) * dec!(0.2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_more_than_limit_short_position(
    ) {
        init_logger();
        let amount_limit = dec!(5);
        let is_reversed = false;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(10),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Sell, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(margin_buy, dec!(5) + dec!(1.9));

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(margin_sell, (dec!(5) - dec!(1.9)) / dec!(0.2) * dec!(0.2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_more_than_limit_short_position_reversed(
    ) {
        init_logger();
        let amount_limit = dec!(5) / BalanceManagerDerivative::price();
        let is_reversed = true;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::btc(),
            dec!(100),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Sell, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(
            margin_buy,
            amount_limit + BalanceManagerDerivative::amount_reversed()
        );

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(
            margin_sell,
            (amount_limit - BalanceManagerDerivative::amount_reversed()) / dec!(0.2) * dec!(0.2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_less_than_limit_long_position(
    ) {
        init_logger();
        let amount_limit = dec!(10);
        let is_reversed = false;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(10),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Buy, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(
            margin_buy,
            (dec!(10) - dec!(1.9)) * dec!(0.2) * dec!(5) * dec!(0.95)
        );

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");

        assert_eq!(
            margin_sell,
            (dec!(10) - dec!(1.9) + dec!(1.9)) * dec!(5) * dec!(0.95) * dec!(0.2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_less_than_limit_long_position_reversed(
    ) {
        init_logger();
        let amount_limit = dec!(1000)
            / BalanceManagerDerivative::price()
            / BalanceManagerDerivative::reversed_amount_multiplier();
        let is_reversed = true;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::btc(),
            dec!(100),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Buy, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(
            margin_buy,
            (dec!(100)
                - BalanceManagerDerivative::amount_reversed() * BalanceManagerDerivative::price()
                    / dec!(5)
                    * BalanceManagerDerivative::reversed_amount_multiplier())
                / BalanceManagerDerivative::price()
                * dec!(5)
                / BalanceManagerDerivative::reversed_amount_multiplier()
                * dec!(0.95)
        );

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(
            margin_sell,
            dec!(100) / BalanceManagerDerivative::price() * dec!(5)
                / BalanceManagerDerivative::reversed_amount_multiplier()
                * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_less_than_limit_short_position(
    ) {
        init_logger();
        let amount_limit = dec!(10);
        let is_reversed = false;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(10),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Sell, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(
            margin_buy,
            (dec!(10) - dec!(1.9) + dec!(1.9)) * dec!(0.2) * dec!(5) * dec!(0.95)
        );

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(
            margin_sell,
            (dec!(10) - dec!(1.9)) * dec!(5) * dec!(0.2) * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_balance_is_less_than_limit_short_position_reversed(
    ) {
        init_logger();
        let amount_limit = dec!(1000)
            / BalanceManagerDerivative::price()
            / BalanceManagerDerivative::reversed_amount_multiplier();
        let is_reversed = true;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::btc(),
            dec!(100),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Sell, None, None, is_reversed);

        let margin_buy = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Buy,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut None,
            )
            .expect("in test");

        assert_eq!(
            margin_buy,
            dec!(100) / BalanceManagerDerivative::price() * dec!(5) * dec!(0.95)
                / BalanceManagerDerivative::reversed_amount_multiplier()
        );

        let margin_sell = test_object
            .balance_manager()
            .get_leveraged_balance_in_amount_currency_code(
                test_object.balance_manager_base.configuration_descriptor,
                OrderSide::Sell,
                test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.symbol(),
                BalanceManagerDerivative::price(),
                &mut Some(Explanation::default()),
            )
            .expect("in test");
        assert_eq!(
            margin_sell,
            (dec!(100)
                - BalanceManagerDerivative::amount_reversed() * BalanceManagerDerivative::price()
                    / dec!(5)
                    * BalanceManagerDerivative::reversed_amount_multiplier())
                / BalanceManagerDerivative::price()
                * dec!(5)
                / BalanceManagerDerivative::reversed_amount_multiplier()
                * dec!(0.95)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn get_leveraged_balance_in_amount_currency_code_max_rounding_error() {
        //real-life case with a rounding error https://github.com/CryptoDreamTeam/CryptoLp/issues/1348
        init_logger();
        let amount_limit = dec!(30);
        let price = dec!(9341);
        let is_reversed = false;

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(100),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol = test_object.balance_manager_base.symbol();
        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(symbol.currency_pair(), dec!(5));

        test_object.fill_order(OrderSide::Sell, Some(price), Some(dec!(20)), is_reversed);
        let balance = dec!(0.0139536399914456800684345595);

        BalanceManagerBase::update_balance(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth() => balance],
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_leveraged_balance_in_amount_currency_code(
                    configuration_descriptor,
                    OrderSide::Sell,
                    exchange_account_id,
                    symbol,
                    price,
                    &mut Some(Explanation::default())
                )
                .expect("in test"),
            dec!(10)
        );
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn update_exchange_balance_should_restore_position_on_all_exchanges(
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let position = dec!(2);

        let test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(0),
            None,
            is_reversed,
            Some(position),
        );

        let exchange_account_id_1 = test_object.balance_manager_base.exchange_account_id_1;
        let exchange_account_id_2 = test_object.balance_manager_base.exchange_account_id_2;
        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id_2,
            hashmap![BalanceManagerBase::eth() => dec!(0)],
            hashmap![symbol_currency_pair => position],
        );

        let positions = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");

        assert_eq!(
            positions
                .get(exchange_account_id_1, symbol_currency_pair)
                .expect("in test"),
            position
        );
        assert_eq!(
            positions
                .get(exchange_account_id_2, symbol_currency_pair)
                .expect("in test"),
            position
        );
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn update_exchange_balance_should_change_fill_position_only_on_first_update(
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let position = dec!(2);

        let test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(0),
            None,
            is_reversed,
            Some(position),
        );

        let exchange_account_id = test_object.balance_manager_base.exchange_account_id_1;
        let symbol_currency_pair = test_object.balance_manager_base.symbol().currency_pair();

        let positions = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");
        assert_eq!(
            positions
                .get(exchange_account_id, symbol_currency_pair)
                .expect("in test"),
            position
        );

        BalanceManagerBase::update_balance_with_positions(
            &mut test_object.balance_manager(),
            exchange_account_id,
            hashmap![BalanceManagerBase::eth() => dec!(1)],
            hashmap![symbol_currency_pair => dec!(3)],
        );

        let positions = test_object
            .balance_manager()
            .get_balances()
            .position_by_fill_amount
            .expect("in test");
        assert_eq!(
            positions
                .get(exchange_account_id, symbol_currency_pair)
                .expect("in test"),
            position
        );
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn reservation_over_limit_should_return_false_on_try_reserve(
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let amount_limit = dec!(2);

        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(100),
            Some(amount_limit),
            is_reversed,
            None,
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::generate());
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(2),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object.balance_manager_base.configuration_descriptor;
        test_object
            .balance_manager()
            .order_was_filled(configuration_descriptor, &order);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.1),
            dec!(1),
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None)
            .is_some());

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            OrderSide::Sell,
            dec!(0.1),
            dec!(4),
        );
        assert!(test_object
            .balance_manager()
            .try_reserve(&reserve_parameters, &mut None,)
            .is_none());
    }
}
