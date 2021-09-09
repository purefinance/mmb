#[cfg(test)]
use std::{collections::HashMap, sync::Arc};

use crate::core::{
    balance_manager::balance_manager::BalanceManager,
    exchanges::{
        common::{Amount, ExchangeAccountId},
        general::{
            currency_pair_metadata::{CurrencyPairMetadata, Precision},
            currency_pair_to_currency_metadata_converter::CurrencyPairToCurrencyMetadataConverter,
            exchange::Exchange,
            test_helper::{
                get_test_exchange_with_currency_pair_metadata,
                get_test_exchange_with_currency_pair_metadata_and_id,
            },
        },
    },
    orders::{
        fill::{OrderFill, OrderFillType},
        order::{OrderFillRole, OrderSide, ReservationId},
    },
};
use chrono::Utc;
use uuid::Uuid;

use crate::core::balance_manager::tests::balance_manager_base::BalanceManagerBase;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct BalanceManagerDerivative {
    balance_manager_base: BalanceManagerBase,
    exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>,
}

// static
impl BalanceManagerDerivative {
    pub fn price() -> Decimal {
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

    fn create_balance_manager(
        is_reversed: bool,
    ) -> (
        Arc<CurrencyPairMetadata>,
        BalanceManager,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let (currency_pair_metadata, exchanges_by_id) =
            BalanceManagerDerivative::create_balance_manager_ctor_parameters(is_reversed);
        let currency_pair_to_currency_pair_metadata_converter =
            CurrencyPairToCurrencyMetadataConverter::new(exchanges_by_id.clone());

        let balance_manager = BalanceManager::new(
            exchanges_by_id.clone(),
            currency_pair_to_currency_pair_metadata_converter,
        );
        (currency_pair_metadata, balance_manager, exchanges_by_id)
    }

    fn create_balance_manager_ctor_parameters(
        is_reversed: bool,
    ) -> (
        Arc<CurrencyPairMetadata>,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let base_currency_code = BalanceManagerBase::eth();
        let quote_currency_code = BalanceManagerBase::btc();

        let balance_currency_code = if is_reversed {
            BalanceManagerBase::btc()
        } else {
            BalanceManagerBase::eth()
        };
        let amount_currency_code = if is_reversed {
            BalanceManagerBase::eth()
        } else {
            BalanceManagerBase::btc()
        };

        let mut currency_pair_metadata = CurrencyPairMetadata::new(
            false,
            true,
            base_currency_code.as_str().into(),
            base_currency_code.clone(),
            quote_currency_code.as_str().into(),
            quote_currency_code.clone(),
            None,
            None,
            amount_currency_code.clone(),
            None,
            None,
            None,
            Some(balance_currency_code),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(0.001) },
        );
        if is_reversed {
            currency_pair_metadata.amount_multiplier = dec!(0.001);
        }
        let currency_pair_metadata = Arc::from(currency_pair_metadata);
        let exchange_1 = get_test_exchange_with_currency_pair_metadata_and_id(
            currency_pair_metadata.clone(),
            &ExchangeAccountId::new(BalanceManagerBase::exchange_name().as_str().into(), 0),
        )
        .0;

        let mut res = HashMap::new();
        res.insert(exchange_1.exchange_account_id.clone(), exchange_1);
        let exchange_2 = get_test_exchange_with_currency_pair_metadata_and_id(
            currency_pair_metadata.clone(),
            &ExchangeAccountId::new(BalanceManagerBase::exchange_name().as_str().into(), 1),
        )
        .0;
        res.insert(exchange_2.exchange_account_id.clone(), exchange_2);
        (currency_pair_metadata, res)
    }

    fn new(is_reversed: bool) -> Self {
        let (currency_pair_metadata, balance_manager, exchanges_by_id) =
            BalanceManagerDerivative::create_balance_manager(is_reversed);
        let mut balance_manager_base = BalanceManagerBase::new();
        balance_manager_base.set_balance_manager(balance_manager);
        balance_manager_base.set_currency_pair_metadata(currency_pair_metadata);
        Self {
            balance_manager_base,
            exchanges_by_id,
        }
    }
    fn create_order_fill(
        price: Decimal,
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
    pub fn balance_manager_mut(&mut self) -> &mut BalanceManager {
        self.balance_manager_base.balance_manager_mut()
    }

    pub fn balance_manager(&self) -> &BalanceManager {
        self.balance_manager_base.balance_manager()
    }

    pub fn fill_order(
        &mut self,
        side: OrderSide,
        price: Option<Decimal>,
        amount: Option<Amount>,
        is_reversed: bool,
    ) {
        let price = match price {
            Some(price) => price,
            None => BalanceManagerDerivative::price(),
        };

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

        let reserve_parameters =
            self.balance_manager_base
                .create_reserve_parameters(Some(side), price, amount);
        let mut reservation_id = ReservationId::default();
        self.balance_manager_mut()
            .try_reserve(&reserve_parameters, &mut reservation_id, &mut None);
        let mut order = self.balance_manager_base.create_order(side, reservation_id);
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            price,
            dec!(0),
            is_reversed,
        ));
        let configuration_descriptor = self.balance_manager_base.configuration_descriptor.clone();
        self.balance_manager_mut()
            .order_was_filled(configuration_descriptor, &order, None);
        self.balance_manager_mut().unreserve(reservation_id, amount);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Utc;
    use rstest::rstest;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::core::balance_manager::balance_manager::BalanceManager;
    use crate::core::balance_manager::position_change::PositionChange;
    use crate::core::exchanges::common::{CurrencyCode, TradePlaceAccount};
    use crate::core::exchanges::general::currency_pair_metadata::{
        CurrencyPairMetadata, Precision,
    };
    use crate::core::exchanges::general::currency_pair_to_currency_metadata_converter::CurrencyPairToCurrencyMetadataConverter;
    use crate::core::logger::init_logger;
    use crate::core::misc::make_hash_map::make_hash_map;
    use crate::core::misc::reserve_parameters::ReserveParameters;
    use crate::core::orders::order::{
        ClientOrderFillId, ClientOrderId, OrderSide, OrderSnapshot, OrderStatus, ReservationId,
    };
    use crate::core::{
        balance_manager::tests::balance_manager_base::BalanceManagerBase,
        exchanges::common::ExchangeAccountId,
    };

    use super::BalanceManagerDerivative;

    fn create_eth_btc_test_obj(
        btc_amount: Decimal,
        eth_amount: Decimal,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        let mut test_object = BalanceManagerDerivative::new(is_reversed);

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        let btc_currency_code = BalanceManagerBase::btc();
        let eth_currency_code = BalanceManagerBase::eth();
        balance_map.insert(btc_currency_code, btc_amount);
        balance_map.insert(eth_currency_code, eth_amount);

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
        );
        test_object
    }

    fn create_test_obj_with_multiple_currencies(
        currency_codes: Vec<CurrencyCode>,
        amounts: Vec<Decimal>,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        if currency_codes.len() != amounts.len() {
            std::panic!("Failed to create test object: currency_codes.len() = {} should be equal amounts.len() = {}",
            currency_codes.len(), amounts.len());
        }
        let mut test_object = BalanceManagerDerivative::new(is_reversed);

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        for i in 0..currency_codes.len() {
            balance_map.insert(
                currency_codes.get(i).expect("in test").clone(),
                amounts.get(i).expect("in test").clone(),
            );
        }

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
        );
        test_object
    }

    fn create_eth_btc_test_obj_for_two_exchanges(
        cc_for_first: CurrencyCode,
        amount_for_first: Decimal,
        cc_for_second: CurrencyCode,
        amount_for_second: Decimal,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        let mut test_object = BalanceManagerDerivative::new(is_reversed);

        let exchange_account_id_1 = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let exchange_account_id_2 = &test_object
            .balance_manager_base
            .exchange_account_id_2
            .clone();

        let mut balance_first_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_first_map.insert(cc_for_first, amount_for_first);
        let mut balance_second_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_second_map.insert(cc_for_second, amount_for_second);

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id_1,
            balance_first_map,
        );

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id_2,
            balance_second_map,
        );
        test_object
    }

    fn create_test_obj_by_currency_code(
        currency_code: CurrencyCode,
        amount: Decimal,
        is_reversed: bool,
    ) -> BalanceManagerDerivative {
        create_test_obj_by_currency_code_with_limit(currency_code, amount, None, is_reversed)
    }

    fn create_test_obj_by_currency_code_with_limit(
        currency_code: CurrencyCode,
        amount: Decimal,
        limit: Option<Decimal>,
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
        amount: Decimal,
        limit: Option<Decimal>,
        is_reversed: bool,
        symbol_currency_pair_amount: Option<Decimal>,
    ) -> BalanceManagerDerivative {
        let mut test_object = BalanceManagerDerivative::new(is_reversed);

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        if let Some(limit) = limit {
            let configuration_descriptor = test_object
                .balance_manager_base
                .configuration_descriptor
                .clone();
            let currency_pair_metadata = test_object
                .balance_manager_base
                .currency_pair_metadata()
                .clone();

            test_object.balance_manager_mut().set_target_amount_limit(
                configuration_descriptor.clone(),
                &exchange_account_id,
                currency_pair_metadata,
                limit,
            );
            let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
                Some(OrderSide::Buy),
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

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_map.insert(currency_code, amount);
        if let Some(symbol_currency_pair_amount) = symbol_currency_pair_amount {
            let symbol_currency_pair = test_object
                .balance_manager_base
                .currency_pair_metadata()
                .currency_pair();
            BalanceManagerBase::update_balance_with_positions(
                test_object.balance_manager_mut(),
                exchange_account_id,
                balance_map,
                make_hash_map(symbol_currency_pair, symbol_currency_pair_amount),
            );
        } else {
            BalanceManagerBase::update_balance(
                test_object.balance_manager_mut(),
                exchange_account_id,
                balance_map,
            );
        }
        test_object
    }

    #[test]
    pub fn reservation_should_use_balance_currency() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), false);

        let mut reservation_id = ReservationId::default();
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            BalanceManagerDerivative::price(),
            dec!(5),
        );
        assert_eq!(
            test_object.balance_manager_mut().try_reserve(
                &reserve_parameters,
                &mut reservation_id,
                &mut None,
            ),
            true
        );

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
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            BalanceManagerDerivative::price(),
            dec!(4),
        );
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut ReservationId::default(),
            &mut None,
        ));

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

    #[test]
    pub fn reservation_should_use_balance_currency_reversed() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), true);

        let mut reservation_id = ReservationId::default();
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            BalanceManagerDerivative::price(),
            dec!(5),
        );
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            BalanceManagerDerivative::price(),
            dec!(4),
        );
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut ReservationId::default(),
            &mut None,
        ));

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

    // TODO: fixme add log checking must contain an error
    #[rstest]
    #[case(OrderSide::Buy, true)]
    #[case(OrderSide::Sell, true)]
    #[case(OrderSide::Buy, false)]
    #[case(OrderSide::Sell, false)]
    pub fn position_more_than_limit_should_log_error(
        #[case] order_side: OrderSide,
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), false);

        let limit = dec!(2);
        let fill_amount = dec!(3);

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object.balance_manager_mut().set_target_amount_limit(
            configuration_descriptor.clone(),
            &exchange_account_id,
            currency_pair_metadata,
            limit,
        );

        let mut order = test_object
            .balance_manager_base
            .create_order(order_side, ReservationId::default());
        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            fill_amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));
        test_object
            .balance_manager_mut()
            .order_was_finished(configuration_descriptor.clone(), &order)
            .expect("in test");
    }

    #[rstest]
    #[case(OrderSide::Buy, dec!(1), true)]
    #[case(OrderSide::Sell, dec!(1),true)]
    #[case(OrderSide::Buy, dec!(1), false)]
    #[case(OrderSide::Sell, dec!(1),false)]
    pub fn fill_should_change_position(
        #[case] order_side: OrderSide,
        #[case] expected_position: Decimal,
        #[case] is_reversed: bool,
    ) {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(order_side, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_position(
                    &test_object.balance_manager_base.exchange_account_id_1,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    order_side,
                )
                .expect("in test"),
            expected_position
        );
    }

    #[test]
    pub fn fill_buy_should_commission_should_be_deducted_from_balance() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), false);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            false,
        ));
        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

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

    #[test]
    pub fn fill_buy_should_commission_should_be_deducted_from_balance_reversed() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), true);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            true,
        ));
        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), dec!(0.1))
                .expect("in test"),
            (dec!(100) / dec!(0.1) + dec!(0.00005) / dec!(0.1)) * dec!(0.95)
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::btc(), dec!(0.1))
                .expect("in test"),
            (dec!(100)
                - dec!(1) * dec!(0.1) / dec!(5)
                    * BalanceManagerDerivative::reversed_amount_multiplier()
                + dec!(0.00005))
                * dec!(0.95)
        );
    }

    #[test]
    pub fn fill_sell_should_commission_should_be_deducted_from_balance() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

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

    #[test]
    pub fn fill_sell_should_commission_should_be_deducted_from_balance_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            dec!(0.1),
            dec!(1),
            dec!(0.1),
            dec!(-0.025) / dec!(100),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), dec!(0.1))
                .expect("in test"),
            (dec!(100) / dec!(0.1)
                - dec!(1) / dec!(5) * BalanceManagerDerivative::reversed_amount_multiplier()
                + dec!(0.00005) / dec!(0.1))
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

    #[test]
    pub fn reservation_after_fill_in_the_same_direction_buy_should_be_not_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            price,
            dec!(1),
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Buy, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
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
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Sell
                )
                .expect("in test"),
            dec!(-1)
        );

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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

    #[test]
    pub fn reservation_after_fill_in_the_same_direction_buy_should_be_not_free_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            price,
            amount,
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Buy, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
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
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Sell
                )
                .expect("in test"),
            -amount
        );

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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

    #[test]
    pub fn reservation_after_fill_in_the_same_direction_sell_should_be_not_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            price,
            dec!(1),
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Sell, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
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
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy
                )
                .expect("in test"),
            dec!(-1)
        );

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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

    #[test]
    pub fn reservation_after_fill_in_the_same_direction_sell_should_be_not_free_reversed() {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            price,
            amount,
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Sell, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
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
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy
                )
                .expect("in test"),
            -amount
        );

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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

    #[test]
    pub fn reservation_after_fill_in_opposite_direction_buy_sell_should_be_partially_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            price,
            dec!(1),
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Buy, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(1))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy
                )
                .expect("in test"),
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
            Some(OrderSide::Sell),
            price,
            dec!(1.5),
        );
        let mut partially_free_reservation_id = ReservationId::default();
        //1 out of 1.5 is free
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut partially_free_reservation_id,
            &mut None,
        ));

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
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .balance_manager_mut()
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

    #[test]
    pub fn reservation_after_fill_in_opposite_direction_buy_sell_should_be_partially_free_reversed()
    {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            price,
            amount,
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Buy, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, amount)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy
                )
                .expect("in test"),
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
            Some(OrderSide::Sell),
            price,
            amount * dec!(1.5),
        );
        let mut partially_free_reservation_id = ReservationId::default();
        //1 out of 1.5 is free
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut partially_free_reservation_id,
            &mut None,
        ));

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
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
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
            .balance_manager_mut()
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

    #[test]
    pub fn reservation_after_fill_in_opposite_direction_sell_buy_should_be_partially_free() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            price,
            dec!(1),
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Sell, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            dec!(1),
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(1))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy
                )
                .expect("in test"),
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
            Some(OrderSide::Buy),
            price,
            dec!(1.5),
        );
        let mut partially_free_reservation_id = ReservationId::default();
        //1 out of 1.5 is free
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut partially_free_reservation_id,
            &mut None,
        ));

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
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .balance_manager_mut()
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

    #[test]
    pub fn reservation_after_fill_in_opposite_direction_sell_buy_should_be_partially_free_reversed()
    {
        init_logger();
        let is_reversed = true;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(100), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.1);
        let amount = dec!(1) / price;
        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            price,
            amount,
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

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
            .create_order(OrderSide::Sell, ReservationId::default());

        order.add_fill(BalanceManagerDerivative::create_order_fill(
            price,
            amount,
            dec!(0.1),
            dec!(0),
            is_reversed,
        ));

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, amount)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_position(
                    &exchange_account_id,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy
                )
                .expect("in test"),
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
            Some(OrderSide::Sell),
            price,
            amount * dec!(1.5),
        );
        let mut partially_free_reservation_id = ReservationId::default();
        //1 out of 1.5 is free
        let partially_reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), price, amount * dec!(1.5));
        assert!(test_object.balance_manager_mut().try_reserve(
            &partially_reserve_parameters,
            &mut partially_free_reservation_id,
            &mut None,
        ));

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
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
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
            .balance_manager_mut()
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

    #[test]
    pub fn clone_when_order_got_status_created_but_its_reservation_is_not_approved_possible_precision_error(
    ) {
        // This case may happen because parallel nature of handling orders

        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(10), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            dec!(0.2),
            dec!(5),
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        let mut order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, reservation_id);
        order_1.set_status(OrderStatus::Created, Utc::now());

        // ApproveReservation wait on lock after Clone started
        let cloned_balance_manager = test_object
            .balance_manager()
            .clone_and_subtract_not_approved_data(Some(vec![order_1.clone()]))
            .expect("in test");
        // TODO: add log checking
        // TestCorrelator.GetLogEventsFromCurrentContext().Should().NotContain(logEvent => logEvent.Level == LogEventLevel.Error || logEvent.Level == LogEventLevel.Fatal);

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_currency_code(BalanceManagerBase::eth(), order_1.price())
                .expect("in test"),
            (dec!(10) - order_1.amount() / order_1.price() / dec!(5)) * dec!(0.95)
        );

        //cloned BalancedManager should be without reservation
        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_another_balance_manager_and_currency_code(
                    &cloned_balance_manager,
                    BalanceManagerBase::eth(),
                    order_1.price()
                )
                .expect("in test"),
            dec!(10) * dec!(0.95)
        );
    }

    #[test]
    pub fn clone_when_order_created() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(10), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(currency_pair_metadata.currency_pair(), dec!(5));

        let price = dec!(0.2);

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            price,
            dec!(5),
        );
        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Buy, reservation_id);
        order.fills.filled_amount = order.amount() / dec!(2);
        order.set_status(OrderStatus::Created, Utc::now());

        test_object.balance_manager_mut().approve_reservation(
            reservation_id,
            &order.header.client_order_id,
            order.amount(),
        );

        // ApproveReservation wait on lock after Clone started
        let cloned_balance_manager = test_object
            .balance_manager()
            .clone_and_subtract_not_approved_data(Some(vec![order.clone()]))
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
                    &cloned_balance_manager,
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
    #[case(dec!(25), dec!(0.5), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )] // Pessimistic case: price1 > price2
    pub fn transfer_reservation_different_price_success(
        #[case] src_balance: Decimal,
        #[case] price_1: Decimal,
        #[case] amount_1: Decimal,
        #[case] price_2: Decimal,
        #[case] amount_2: Decimal,
        #[case] amount_to_transfer: Decimal,
    ) {
        init_logger();
        let is_reversed = false;
        let mut test_object = create_eth_btc_test_obj(src_balance, src_balance, is_reversed);

        let side = OrderSide::Sell;

        let common_params = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(side), price_1, dec!(0))
            .clone();
        let initial_balance = test_object
            .balance_manager()
            .get_balance_by_reserve_parameters(&common_params)
            .expect("in test");

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(side), price_1, amount_1)
            .clone();
        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));
        let reservation_1 = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test")
            .clone();
        let balance_1 = initial_balance
            - reservation_1
                .convert_in_reservation_currency(reservation_1.amount)
                .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&common_params),
            Some(balance_1)
        );

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(side), price_2, amount_2)
            .clone();
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));
        let reservation_2 = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test")
            .clone();
        let balance_2 = balance_1
            - reservation_2
                .convert_in_reservation_currency(reservation_2.amount)
                .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&common_params),
            Some(balance_2)
        );

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            amount_to_transfer,
            &None
        ));

        let add = reservation_1
            .convert_in_reservation_currency(amount_to_transfer)
            .expect("in test");
        let sub = reservation_2
            .convert_in_reservation_currency(amount_to_transfer)
            .expect("in test");
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&common_params),
            Some(balance_2 + add - sub)
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(3) - dec!(2));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(2) + dec!(2));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[rstest]
    #[ignore] // Transfer
    #[case(dec!(20), dec!(0.5), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )] // Pessimistic case: price1 > price2
    pub fn transfer_reservation_different_price_failure(
        #[case] src_balance: Decimal,
        #[case] price_1: Decimal,
        #[case] amount_1: Decimal,
        #[case] price_2: Decimal,
        #[case] amount_2: Decimal,
        #[case] amount_to_transfer: Decimal,
    ) {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), src_balance, is_reversed);

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), price_1, amount_1)
            .clone();
        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));
        let balance_1 = src_balance - amount_1 / price_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_1)
        );

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), price_2, amount_2)
            .clone();
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));
        let balance_2 = balance_1 - amount_2 / price_2;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(balance_2)
        );

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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

    #[test]
    #[ignore] // Transfer
    pub fn transfer_reservations_amount_partial() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(30), is_reversed);

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(3))
            .clone();
        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(2))
            .clone();
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));

        let expected_balance = dec!(5);
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(expected_balance)
        );

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[test]
    #[ignore] // Transfer
    pub fn transfer_reservations_amount_partial_with_cost_diff_due_to_fill() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(25), is_reversed);

        let price = dec!(0.2);

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), price, dec!(3))
            .clone();
        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation(reservation_id_1)
                .expect("in test")
                .cost,
            dec!(3)
        );

        let buy_reservation_params = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), price, dec!(1))
            .clone();

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&buy_reservation_params),
            Some(dec!(25) * price - dec!(3))
        );

        let mut buy_reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &buy_reservation_params,
            &mut buy_reservation_id,
            &mut None,
        ));

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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            configuration_descriptor.clone(),
            &order,
            None,
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_position(
                    &test_object.balance_manager_base.exchange_account_id_1,
                    &test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .currency_pair(),
                    OrderSide::Buy,
                )
                .expect("in test"),
            dec!(1)
        );
        test_object
            .balance_manager_mut()
            .unreserve(buy_reservation_id, order.amount())
            .expect("in test");

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), price, dec!(1.9))
            .clone();
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation(reservation_id_1)
                .expect("in test")
                .cost,
            dec!(0.9)
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0.5))
        );

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.cost, dec!(3) - dec!(2));

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");
        assert_eq!(reservation.amount, dec!(1.9) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(1.9) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(1.9) + dec!(2));
        assert_eq!(reservation.cost, dec!(0.9) + dec!(2));
    }

    #[test]
    pub fn update_exchange_balance_should_use_cost_for_balance_filter_when_no_free_cost() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(25), is_reversed);

        let price = dec!(0.2);

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), price, dec!(2))
            .clone();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut ReservationId::default(),
            &mut None,
        ));

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some((dec!(25) - dec!(2) / price) * dec!(0.95))
        );
        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        assert_eq!(
            test_object
                .balance_manager()
                .get_exchange_balance(
                    &exchange_account_id,
                    test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .clone(),
                    &BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25)
        );

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_map.insert(BalanceManagerBase::eth(), dec!(25));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            &exchange_account_id,
            balance_map.clone(),
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
                    &exchange_account_id,
                    test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .clone(),
                    &BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25) - dec!(2) / price
        );
    }

    #[test]
    pub fn update_exchange_balance_should_use_cost_for_balance_filter_when_partially_free_cost() {
        init_logger();
        let is_reversed = false;
        let mut test_object = create_test_obj_by_currency_code_and_symbol_currency_pair(
            BalanceManagerBase::eth(),
            dec!(25),
            None,
            is_reversed,
            Some(dec!(1)),
        );

        let price = dec!(0.2);

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), price, dec!(2))
            .clone();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut ReservationId::default(),
            &mut None,
        ));

        assert_eq!(
            test_object
                .balance_manager_base
                .get_balance_by_trade_side(OrderSide::Sell, price),
            Some((dec!(25) - (dec!(2) - dec!(1)) / price) * dec!(0.95))
        );
        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        assert_eq!(
            test_object
                .balance_manager()
                .get_exchange_balance(
                    &exchange_account_id,
                    test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .clone(),
                    &BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25)
        );

        let symbol_currency_pair = test_object
            .balance_manager_base
            .currency_pair_metadata()
            .currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            test_object.balance_manager_mut(),
            &exchange_account_id,
            make_hash_map(BalanceManagerBase::eth(), dec!(25)),
            make_hash_map(symbol_currency_pair, dec!(1)),
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
                    &exchange_account_id,
                    test_object
                        .balance_manager_base
                        .currency_pair_metadata()
                        .clone(),
                    &BalanceManagerBase::eth(),
                )
                .expect("in test"),
            dec!(25) - (dec!(2) - dec!(1)) / price
        );
    }

    #[test]
    pub fn fills_and_reservations_no_limit_buy_enough_and_not_enough() {
        init_logger();
        let is_reversed = false;
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(10), is_reversed);

        let exchange_account_id = test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let currency_pair_metadata = test_object.balance_manager_base.currency_pair_metadata();

        let price = BalanceManagerDerivative::price();
        test_object
            .exchanges_by_id
            .get_mut(&exchange_account_id)
            .expect("in test")
            .leverage_by_currency_pair
            .insert(
                currency_pair_metadata.currency_pair(),
                BalanceManagerDerivative::leverage(),
            );

        let original_balance = dec!(9);
        let position = dec!(1);

        let symbol_currency_pair = test_object
            .balance_manager_base
            .currency_pair_metadata()
            .currency_pair();
        BalanceManagerBase::update_balance_with_positions(
            test_object.balance_manager_mut(),
            &exchange_account_id,
            make_hash_map(BalanceManagerBase::eth(), original_balance),
            make_hash_map(symbol_currency_pair, position),
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

    // public void Reservation_Should_UseBalanceCurrency()
    // {

    //     BalanceManager.TryReserve(CreateReserveParams(OrderSide.Sell, Price, 5), out var sellReservationId).Should().BeTrue();
    //     GetBalance(Eth, Price).Should().Be((100 - 5 / Price) * 0.95m);
    //     BalanceManager.UnReserve(sellReservationId, 5);

    //     BalanceManager.TryReserve(CreateReserveParams(OrderSide.Buy, Price, 4), out _).Should().BeTrue();
    //     GetBalance(Eth, Price).Should().Be((100 - 4 / Price) * 0.95m);
    // }
}
