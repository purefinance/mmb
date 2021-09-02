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
            test_helper::get_test_exchange_with_currency_pair_metadata_and_id,
        },
    },
    orders::{
        fill::{OrderFill, OrderFillType},
        order::OrderFillRole,
    },
    DateTime,
};
use chrono::Utc;
use uuid::Uuid;

use crate::core::balance_manager::tests::balance_manager_base::BalanceManagerBase;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct BalanceManagerOrdinal {
    pub balance_manager_base: BalanceManagerBase,
    now: DateTime,
}

// static
impl BalanceManagerOrdinal {
    fn create_balance_manager() -> (Arc<CurrencyPairMetadata>, BalanceManager) {
        let (currency_pair_metadata, exchanges_by_id) =
            BalanceManagerOrdinal::create_balance_manager_ctor_parameters();
        let currency_pair_to_currency_pair_metadata_converter =
            CurrencyPairToCurrencyMetadataConverter::new(exchanges_by_id.clone());

        let balance_manager = BalanceManager::new(
            exchanges_by_id.clone(),
            currency_pair_to_currency_pair_metadata_converter,
        );
        (currency_pair_metadata, balance_manager)
    }

    fn create_balance_manager_ctor_parameters() -> (
        Arc<CurrencyPairMetadata>,
        HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) {
        let base_currency_code = BalanceManagerBase::eth();
        let quote_currency_code = BalanceManagerBase::btc();
        let currency_pair_metadata = Arc::from(CurrencyPairMetadata::new(
            false,
            false,
            base_currency_code.as_str().into(),
            base_currency_code.clone(),
            quote_currency_code.as_str().into(),
            quote_currency_code,
            None,
            None,
            base_currency_code.clone(),
            None,
            None,
            None,
            Some(base_currency_code),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(0.001) },
        ));

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

    fn new() -> Self {
        let (currency_pair_metadata, balance_manager) =
            BalanceManagerOrdinal::create_balance_manager();
        let mut balance_manager_base = BalanceManagerBase::new();
        balance_manager_base.set_balance_manager(balance_manager);
        balance_manager_base.set_currency_pair_metadata(currency_pair_metadata);
        Self {
            balance_manager_base,
            now: Utc::now(),
        }
    }
}

impl BalanceManagerOrdinal {
    fn create_order_fill(price: Decimal, amount: Amount, cost: Decimal) -> OrderFill {
        OrderFill::new(
            Uuid::new_v4(),
            Utc::now(),
            OrderFillType::UserTrade,
            None,
            price,
            amount,
            cost,
            OrderFillRole::Taker,
            BalanceManagerBase::bnb(),
            dec!(0.1),
            dec!(0.1),
            BalanceManagerBase::bnb(),
            dec!(0.1),
            dec!(0.1),
            false,
            None,
            None,
        )
    }

    pub fn balance_manager(&self) -> &BalanceManager {
        self.balance_manager_base.balance_manager()
    }
    pub fn balance_manager_mut(&mut self) -> &mut BalanceManager {
        self.balance_manager_base.balance_manager_mut()
    }
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use rstest::rstest;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use crate::core::exchanges::common::CurrencyCode;
    use crate::core::exchanges::general::currency_pair_metadata::{
        CurrencyPairMetadata, Precision,
    };
    use crate::core::logger::init_logger;
    use crate::core::misc::reserve_parameters::ReserveParameters;
    use crate::core::orders::order::{ClientOrderId, OrderSide, ReservationId};
    use crate::core::{
        balance_manager::tests::balance_manager_base::BalanceManagerBase,
        exchanges::common::ExchangeAccountId,
    };

    use super::BalanceManagerOrdinal;

    fn create_eth_btc_test_obj(btc_amount: Decimal, eth_amount: Decimal) -> BalanceManagerOrdinal {
        let mut test_object = BalanceManagerOrdinal::new();

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
    ) -> BalanceManagerOrdinal {
        if currency_codes.len() != amounts.len() {
            std::panic!("Failed to create test object: currency_codes.len() = {} should be equal amounts.len() = {}",
            currency_codes.len(), amounts.len());
        }
        let mut test_object = BalanceManagerOrdinal::new();

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
    ) -> BalanceManagerOrdinal {
        let mut test_object = BalanceManagerOrdinal::new();

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
    ) -> BalanceManagerOrdinal {
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_map.insert(currency_code, amount);

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
        );
        test_object
    }

    #[test]
    pub fn balance_was_received_not_existing_exchange_account_id() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();
        assert_eq!(
            test_object
                .balance_manager()
                .balance_was_received(&ExchangeAccountId::new("NotExistingExchangeId".into(), 0)),
            false
        );
    }

    #[test]
    pub fn balance_was_received_existing_exchange_account_id_without_currency() {
        init_logger();
        let test_object = BalanceManagerOrdinal::new();
        assert_eq!(
            test_object
                .balance_manager()
                .balance_was_received(&test_object.balance_manager_base.exchange_account_id_1),
            false
        );
    }

    #[test]
    pub fn balance_was_received_existing_exchange_account_id_with_currency() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(2));

        assert!(test_object
            .balance_manager()
            .balance_was_received(&test_object.balance_manager_base.exchange_account_id_1));
    }

    #[test]
    pub fn update_exchange_balance_skip_currencies_with_zero_balance_which_are_not_part_of_currency_pairs(
    ) {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();
        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        let btc_currency_code: CurrencyCode = BalanceManagerBase::btc().as_str().into();
        let eth_currency_code: CurrencyCode = BalanceManagerBase::eth().as_str().into();
        let bnb_currency_code: CurrencyCode = BalanceManagerBase::bnb().as_str().into();
        let eos_currency_code: CurrencyCode = "EOS".into();
        balance_map.insert(btc_currency_code.clone(), dec!(2));
        balance_map.insert(eth_currency_code.clone(), dec!(1));
        balance_map.insert(bnb_currency_code.clone(), dec!(7.5));
        balance_map.insert(eos_currency_code.clone(), dec!(0));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &btc_currency_code
            ),
            Some(dec!(2))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &eth_currency_code
            ),
            Some(dec!(1))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &bnb_currency_code
            ),
            Some(dec!(7.5))
        );

        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &eos_currency_code
            ),
            None
        );
    }

    #[test]
    pub fn get_balance_buy_returns_quote_balance_and_currency_code() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(0.5), dec!(0.1));
        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let trade_side = OrderSide::Buy;

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_reservation_currency_code(
                    test_object.balance_manager_base.currency_pair_metadata(),
                    trade_side,
                ),
            BalanceManagerBase::btc()
        );

        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                &test_object.balance_manager_base.configuration_descriptor,
                &exchange_account_id,
                test_object
                    .balance_manager_base
                    .currency_pair_metadata()
                    .clone(),
                trade_side,
                dec!(1),
            ),
            Some(dec!(0.5))
        );
    }

    #[test]
    pub fn get_balance_sell_return_base_balance_and_currency_code() {
        init_logger();
        let test_object = create_eth_btc_test_obj(dec!(0.5), dec!(0.1));
        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let trade_side = OrderSide::Sell;

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_reservation_currency_code(
                    test_object.balance_manager_base.currency_pair_metadata(),
                    trade_side,
                ),
            BalanceManagerBase::eth()
        );

        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                &test_object.balance_manager_base.configuration_descriptor,
                &exchange_account_id,
                test_object
                    .balance_manager_base
                    .currency_pair_metadata()
                    .clone(),
                trade_side,
                dec!(1),
            ),
            Some(dec!(0.1))
        );
    }

    #[test]
    pub fn can_reserve_buy_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
            dec!(0.2),
            dec!(5),
        );

        assert_eq!(
            test_object
                .balance_manager()
                .can_reserve(&reserve_parameters, &mut None),
            false
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );
    }

    #[test]
    pub fn can_reserve_buy_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Buy),
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

    #[test]
    pub fn can_reserve_sell_not_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0.5));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
            dec!(0.2),
            dec!(5),
        );

        assert_eq!(
            test_object
                .balance_manager()
                .can_reserve(&reserve_parameters, &mut None),
            false
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );
    }

    #[test]
    pub fn can_reserve_sell_enough_balance() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5.0));

        let reserve_parameters = test_object.balance_manager_base.create_reserve_parameters(
            Some(OrderSide::Sell),
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

    #[test]
    pub fn try_reserve_buy_not_enough_balance() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(0.5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(!test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[test]
    pub fn try_reserve_buy_enough_balance() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Buy));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());
    }

    #[test]
    pub fn try_reserve_sell_not_enough_balance() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(0.5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(!test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.5))
        );

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[test]
    pub fn try_reserve_sell_enough_balance() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Sell));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());
    }

    #[test]
    pub fn try_update_reservation_buy_worse_price_not_enough_balance() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        assert_eq!(
            test_object
                .balance_manager_mut()
                .try_update_reservation(reservation_id, dec!(0.3)),
            false
        );
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.1))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[test]
    pub fn try_update_reservation_buy_worse_price_enough_balance() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        assert!(test_object
            .balance_manager_mut()
            .try_update_reservation(reservation_id, dec!(0.3)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");
        assert_eq!(reservation.price, dec!(0.3));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[test]
    pub fn try_update_reservation_buy_better_price() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1.1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.1))
        );

        assert!(test_object
            .balance_manager_mut()
            .try_update_reservation(reservation_id, dec!(0.1)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.6))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");
        assert_eq!(reservation.price, dec!(0.1));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[test]
    pub fn try_update_reservation_sell() {
        init_logger();
        let mut test_object =
            create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5.0));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));
        assert!(test_object
            .balance_manager_mut()
            .try_update_reservation(reservation_id, dec!(0.1)));
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.0))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");
        assert_eq!(reservation.price, dec!(0.1));
        assert_eq!(reservation.not_approved_amount, dec!(5));
    }

    #[test]
    pub fn try_reserve_pair_not_enough_balance_for_1() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(0.0), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();

        assert!(!test_object.balance_manager_mut().try_reserve_pair(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            None
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

    #[test]
    pub fn try_reserve_pair_not_enough_balance_for_2() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(3), dec!(0));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();

        assert!(!test_object.balance_manager_mut().try_reserve_pair(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
        ));

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
            None
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

    #[test]
    pub fn try_reserve_pair_enough_balance() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(1), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();

        assert!(test_object.balance_manager_mut().try_reserve_pair(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
        ));

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

        if reservation_id_1 == ReservationId::default()
            || reservation_id_2 == ReservationId::default()
        {
            assert!(false);
        }

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Buy));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Sell));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());
    }

    #[test]
    pub fn try_reserve_three_not_enough_balance_for_1() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(0.0), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(4))
            .clone();

        let reserve_parameters_3 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(1))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();
        let mut reservation_id_3 = ReservationId::default();

        assert!(!test_object.balance_manager_mut().try_reserve_three(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            reserve_parameters_3.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
            &mut reservation_id_3,
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            None
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(5))
        );

        assert!(
            test_object
                .balance_manager()
                .get_reservation(reservation_id_1)
                .is_none()
                || !test_object
                    .balance_manager()
                    .get_reservation(reservation_id_2)
                    .is_none()
                || !test_object
                    .balance_manager()
                    .get_reservation(reservation_id_3)
                    .is_none()
        );
    }

    #[test]
    pub fn try_reserve_three_not_enough_balance_for_2() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(1), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(6))
            .clone();

        let reserve_parameters_3 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(1))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();
        let mut reservation_id_3 = ReservationId::default();

        assert!(!test_object.balance_manager_mut().try_reserve_three(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            reserve_parameters_3.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
            &mut reservation_id_3,
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
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(5))
        );

        assert!(
            test_object
                .balance_manager()
                .get_reservation(reservation_id_1)
                .is_none()
                || !test_object
                    .balance_manager()
                    .get_reservation(reservation_id_2)
                    .is_none()
                || !test_object
                    .balance_manager()
                    .get_reservation(reservation_id_3)
                    .is_none()
        );
    }

    #[test]
    pub fn try_reserve_three_not_enough_balance_for_3() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(1), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_3 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(1))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();
        let mut reservation_id_3 = ReservationId::default();

        assert!(!test_object.balance_manager_mut().try_reserve_three(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            reserve_parameters_3.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
            &mut reservation_id_3,
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
                .get_balance_by_reserve_parameters(&reserve_parameters_3),
            Some(dec!(5))
        );

        assert!(
            test_object
                .balance_manager()
                .get_reservation(reservation_id_1)
                .is_none()
                || !test_object
                    .balance_manager()
                    .get_reservation(reservation_id_2)
                    .is_none()
                || !test_object
                    .balance_manager()
                    .get_reservation(reservation_id_3)
                    .is_none()
        );
    }

    #[test]
    pub fn try_reserve_three_enough_balance() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(dec!(1), dec!(6));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_3 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(1))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();
        let mut reservation_id_3 = ReservationId::default();

        assert!(test_object.balance_manager_mut().try_reserve_three(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            reserve_parameters_3.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
            &mut reservation_id_3,
        ));

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

        if test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .is_none()
            || test_object
                .balance_manager()
                .get_reservation(reservation_id_2)
                .is_none()
            || test_object
                .balance_manager()
                .get_reservation(reservation_id_3)
                .is_none()
        {
            assert!(false);
        }

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Buy));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Sell));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(5));
        assert_eq!(reservation.not_approved_amount, dec!(5));
        assert_eq!(reservation.unreserved_amount, dec!(5));
        assert!(reservation.approved_parts.is_empty());

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_3)
            .expect("in test");

        assert_eq!(
            reservation.exchange_account_id,
            test_object.balance_manager_base.exchange_account_id_1
        );
        assert_eq!(
            reservation.currency_pair_metadata,
            test_object.balance_manager_base.currency_pair_metadata()
        );
        assert_eq!(reservation.order_side, Some(OrderSide::Sell));
        assert_eq!(reservation.price, dec!(0.2));
        assert_eq!(reservation.amount, dec!(1));
        assert_eq!(reservation.not_approved_amount, dec!(1));
        assert_eq!(reservation.unreserved_amount, dec!(1));
        assert!(reservation.approved_parts.is_empty());
    }

    #[test]
    pub fn unreserve_should_not_unreserve_for_unknown_exchange_account_id() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .get_mut_reservation(reservation_id)
            .expect("in test")
            .exchange_account_id = ExchangeAccountId::new("unknown_id".into(), 0);

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");

        assert_eq!(reservation.unreserved_amount, dec!(5));
    }

    #[test]
    pub fn unreserve_can_unreserve_more_than_reserved_with_compensation_amounts() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
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

    #[test]
    pub fn unreserve_can_not_unreserve_after_complete_unreserved() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(5))
            .expect("in test");

        match test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(5))
        {
            Ok(_) => assert!(false),
            Err(error) => {
                if !error.to_string().contains("Can't find reservation_id=") {
                    assert!(false, "{:?}", error)
                }
            }
        };

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[rstest]
    #[case(dec!(0))]
    // min positive value in rust_decimaL::Decimal (Scale maximum precision - 28)
    #[case(dec!(1e-28))]
    pub fn unreserve_zero_amount(#[case] amount_to_unreserve: Decimal) {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let currency_pair_metadata = Arc::from(CurrencyPairMetadata::new(
            false,
            false,
            BalanceManagerBase::eth().as_str().into(),
            BalanceManagerBase::eth(),
            BalanceManagerBase::btc().as_str().into(),
            BalanceManagerBase::btc(),
            None,
            None,
            BalanceManagerBase::eth(),
            Some(dec!(1)),
            None,
            None,
            Some(BalanceManagerBase::btc()),
            Precision::ByTick { tick: dec!(0.1) },
            Precision::ByTick { tick: dec!(1) },
        ));

        let reserve_parameters = ReserveParameters::new(
            test_object
                .balance_manager_base
                .configuration_descriptor
                .clone(),
            test_object
                .balance_manager_base
                .exchange_account_id_1
                .clone(),
            currency_pair_metadata.clone(),
            Some(OrderSide::Sell),
            dec!(0.2),
            dec!(1),
        );

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        // TODO: add cehcking that log didn't contain Err lvl messages
        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, amount_to_unreserve)
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(4))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");

        assert_eq!(reservation.unreserved_amount, dec!(1));
        assert_eq!(reservation.not_approved_amount, dec!(1));
    }

    #[test]
    pub fn unreserve_buy() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(0.8))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");

        assert_eq!(reservation.unreserved_amount, dec!(1));
    }

    #[test]
    pub fn unreserve_sell() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters),
            Some(dec!(4))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .expect("in test");

        assert_eq!(reservation.unreserved_amount, dec!(1));
    }

    #[test]
    pub fn unreserve_rest_buy() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
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

    #[test]
    pub fn unreserve_rest_sell() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
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

    #[test]
    pub fn unreserve_rest_partially_unreserved_buy() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(1));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        test_object
            .balance_manager_mut()
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

    #[test]
    pub fn unreserve_rest_partially_unreserved_sell() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(5))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(4))
            .expect("in test");

        test_object
            .balance_manager_mut()
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
    pub fn transfer_reservation_different_price_sell(
        #[case] src_balance: Decimal,
        #[case] price_1: Decimal,
        #[case] amount_1: Decimal,
        #[case] price_2: Decimal,
        #[case] amount_2: Decimal,
        #[case] amount_to_transfer: Decimal,
    ) {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(src_balance, src_balance);

        let side = OrderSide::Sell;

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
        let balance_1 = src_balance - amount_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
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
        let balance_2 = balance_1 - amount_2;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
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
            Some(balance_2 - amount_to_transfer + amount_to_transfer)
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(1));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(4));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[rstest]
    #[case(dec!(5), dec!(0.2), dec!(3), dec!(0.5), dec!(2) ,dec!(2) )]
    #[case(dec!(5), dec!(0.2), dec!(3), dec!(0.2), dec!(2) ,dec!(2) )]
    pub fn transfer_reservation_different_price_buy(
        #[case] src_balance: Decimal,
        #[case] price_1: Decimal,
        #[case] amount_1: Decimal,
        #[case] price_2: Decimal,
        #[case] amount_2: Decimal,
        #[case] amount_to_transfer: Decimal,
    ) {
        init_logger();
        let mut test_object = create_eth_btc_test_obj(src_balance, src_balance);

        let side = OrderSide::Buy;

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
        let balance_1 = src_balance - amount_1 * price_1;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
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
        let balance_2 = balance_1 - amount_2 * price_2;
        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_2),
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
            Some(balance_2 + amount_to_transfer * price_1 - amount_to_transfer * price_2)
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(1));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(4));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[test]
    pub fn transfer_reservations_amount_partial() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(3))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(2))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));

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
            Some(dec!(0))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(1));
        assert_eq!(reservation.amount, dec!(3) - dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(3) - dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(3) - dec!(2));

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(4));
        assert_eq!(reservation.amount, dec!(2) + dec!(2));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(2));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(2));
    }

    #[test]
    pub fn transfer_reservations_amount_all() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(3))
            .clone();

        let reserve_parameters_2 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(2))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

        assert_eq!(reservation.cost, dec!(2) + dec!(3));
        assert_eq!(reservation.amount, dec!(2) + dec!(3));
        assert_eq!(reservation.not_approved_amount, dec!(2) + dec!(3));
        assert_eq!(reservation.unreserved_amount, dec!(2) + dec!(3));
    }

    #[test]
    pub fn transfer_reservations_amount_more_than_we_have_should_do_nothing_and_panic() {
        init_logger();
        let test_object = Arc::new(parking_lot::Mutex::new(create_test_obj_by_currency_code(
            BalanceManagerBase::eth(),
            dec!(5),
        )));

        let reserve_parameters_1 = test_object
            .lock()
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(3))
            .clone();

        let reserve_parameters_2 = test_object
            .lock()
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(2))
            .clone();

        let mut reservation_id_1 = ReservationId::default();
        assert!(test_object.lock().balance_manager_mut().try_reserve(
            &reserve_parameters_1,
            &mut reservation_id_1,
            &mut None,
        ));
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.lock().balance_manager_mut().try_reserve(
            &reserve_parameters_2,
            &mut reservation_id_2,
            &mut None,
        ));
        let test_object_clone = test_object.clone();

        let handle = std::thread::spawn(move || {
            test_object
                .lock()
                .balance_manager_mut()
                .try_transfer_reservation(reservation_id_1, reservation_id_2, dec!(5), &None);
        });

        if let Ok(_) = handle.join() {
            assert!(false);
        }

        assert_eq!(
            test_object_clone
                .lock()
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        assert_eq!(
            test_object_clone
                .lock()
                .balance_manager()
                .get_reservation(reservation_id_1)
                .expect("in test")
                .unreserved_amount,
            dec!(3)
        );
        assert_eq!(
            test_object_clone
                .lock()
                .balance_manager()
                .get_reservation(reservation_id_2)
                .expect("in test")
                .unreserved_amount,
            dec!(2)
        );
    }

    #[test]
    pub fn unreserve_zero_from_zero_reservation_should_remove_reservation() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Sell), dec!(0.2), dec!(0))
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id, dec!(0))
            .expect("in test");

        assert!(test_object
            .balance_manager()
            .get_reservation(reservation_id)
            .is_none());
    }

    #[test]
    pub fn transfer_reservations_amount_with_unreserve() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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

        test_object
            .balance_manager_mut()
            .unreserve(reservation_id_1, dec!(1))
            .expect("in test");

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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
                .get_reservation(reservation_id_1)
                .expect("in test")
                .unreserved_amount,
            dec!(3) - dec!(1) - dec!(1)
        );

        assert_eq!(
            test_object
                .balance_manager()
                .get_reservation(reservation_id_2)
                .expect("in test")
                .unreserved_amount,
            dec!(2) + dec!(1)
        );
    }

    #[test]
    pub fn transfer_reservations_amount_partial_approve() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        let amount = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in tests")
            .amount;
        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order.header.client_order_id,
            amount,
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in tests");
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

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(order.header.client_order_id.clone())
        ));

        assert_eq!(
            test_object
                .balance_manager()
                .get_balance_by_reserve_parameters(&reserve_parameters_1),
            Some(dec!(0))
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in tests");
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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

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

    #[test]
    #[should_panic(expected = "failed to update src unreserved amount")]
    pub fn transfer_reservations_amount_more_thane_we_have() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        let amount = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in tests")
            .amount;
        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order.header.client_order_id,
            amount,
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in tests");
        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            reservation.amount
        );

        test_object.balance_manager_mut().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(4),
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "failed to update src unreserved amount")]
    pub fn transfer_reservations_amount_more_than_we_have_by_approve_client_order_id() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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

        let order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order.header.client_order_id,
            dec!(1),
        );

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in tests");
        assert_eq!(
            reservation
                .approved_parts
                .get(&order.header.client_order_id)
                .expect("in test")
                .amount,
            dec!(1)
        );

        test_object.balance_manager_mut().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(order.header.client_order_id.clone()),
        );
    }

    #[test]
    #[should_panic(expected = "failed to update src unreserved amount")]
    pub fn transfer_reservations_unknown_client_order_id() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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
        test_object.balance_manager_mut().try_transfer_reservation(
            reservation_id_1,
            reservation_id_2,
            dec!(2),
            &Some(ClientOrderId::new("unknown_id".into())),
        );
    }

    #[test]
    pub fn transfer_reservations_amount_partial_approve_with_multiple_orders() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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

        let order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());
        let order_2 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order_1.header.client_order_id,
            dec!(1),
        );

        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order_2.header.client_order_id,
            dec!(2),
        );

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

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

    #[test]
    pub fn transfer_reservations_amount_partial_approve_with_multiple_orders_to_existing_part() {
        init_logger();
        let mut test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

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

        let order_1 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());
        let order_2 = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());

        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order_1.header.client_order_id,
            dec!(1),
        );

        test_object.balance_manager_mut().approve_reservation(
            reservation_id_1,
            &order_2.header.client_order_id,
            dec!(2),
        );

        test_object.balance_manager_mut().approve_reservation(
            reservation_id_2,
            &order_2.header.client_order_id,
            dec!(1),
        );

        assert!(test_object.balance_manager_mut().try_transfer_reservation(
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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_1)
            .expect("in test");

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

        let reservation = test_object
            .balance_manager()
            .get_reservation(reservation_id_2)
            .expect("in test");

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

    #[test]
    pub fn unreserve_pair() {
        init_logger();
        let mut test_object = create_eth_btc_test_obj_for_two_exchanges(
            BalanceManagerBase::btc(),
            dec!(1),
            BalanceManagerBase::eth(),
            dec!(5),
        );

        let reserve_parameters_1 = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        let reserve_parameters_2 = ReserveParameters::new(
            test_object
                .balance_manager_base
                .configuration_descriptor
                .clone(),
            test_object
                .balance_manager_base
                .exchange_account_id_2
                .clone(),
            test_object
                .balance_manager_base
                .currency_pair_metadata()
                .clone(),
            Some(OrderSide::Sell),
            dec!(0.2),
            dec!(5),
        );

        let mut reservation_id_1 = ReservationId::default();
        let mut reservation_id_2 = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve_pair(
            reserve_parameters_1.clone(),
            reserve_parameters_2.clone(),
            &mut reservation_id_1,
            &mut reservation_id_2,
        ));

        test_object.balance_manager_mut().unreserve_pair(
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

    #[test]
    pub fn get_balance_not_existing_exchange_account_id() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::eth(), dec!(5));

        assert_eq!(
            test_object.balance_manager().get_balance_by_side(
                &test_object.balance_manager_base.configuration_descriptor,
                &ExchangeAccountId::new("unknown_id".into(), 0),
                test_object.balance_manager_base.currency_pair_metadata(),
                OrderSide::Buy,
                dec!(1),
            ),
            None
        );
    }

    #[test]
    pub fn get_balance_not_existing_currency_code() {
        init_logger();
        let test_object = create_test_obj_by_currency_code(BalanceManagerBase::btc(), dec!(2));

        assert_eq!(
            test_object.balance_manager().get_balance_by_currency_code(
                &test_object.balance_manager_base.configuration_descriptor,
                &test_object.balance_manager_base.exchange_account_id_1,
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::eth(),
                dec!(1),
            ),
            None
        );
    }

    #[test]
    pub fn get_balance_unapproved_reservations_are_counted_even_after_balance_update() {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(2));
        balance_map.insert(BalanceManagerBase::eth(), dec!(0.5));
        balance_map.insert(BalanceManagerBase::bnb(), dec!(0.1));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map.clone(),
        );

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), dec!(5))
            .clone();

        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut ReservationId::default(),
            &mut None,
        ));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
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
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::eth()
            ),
            Some(dec!(0.5))
        );
        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::bnb()
            ),
            Some(dec!(0.1))
        );
    }

    #[test]
    pub fn get_balance_approved_reservations_are_not_counted_after_balance_update() {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(2));
        balance_map.insert(BalanceManagerBase::eth(), dec!(0.5));
        balance_map.insert(BalanceManagerBase::bnb(), dec!(0.1));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map.clone(),
        );

        let amount = dec!(5);
        let client_order_id = ClientOrderId::unique_id();

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), dec!(0.2), amount)
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object.balance_manager_mut().approve_reservation(
            reservation_id,
            &client_order_id,
            amount,
        );

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map,
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
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::eth()
            ),
            Some(dec!(0.5))
        );
        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::bnb()
            ),
            Some(dec!(0.1))
        );
    }

    #[test]
    pub fn get_balance_partially_approved_reservations_are_not_counted_after_balance_update() {
        init_logger();
        let mut test_object = BalanceManagerOrdinal::new();

        let exchange_account_id = &test_object
            .balance_manager_base
            .exchange_account_id_1
            .clone();

        let mut balance_map: HashMap<CurrencyCode, Decimal> = HashMap::new();
        balance_map.insert(BalanceManagerBase::btc(), dec!(2));
        balance_map.insert(BalanceManagerBase::eth(), dec!(0.5));
        balance_map.insert(BalanceManagerBase::bnb(), dec!(0.1));

        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
            exchange_account_id,
            balance_map.clone(),
        );

        let amount = dec!(5);
        let price = dec!(0.2);
        let approved_amount = amount / dec!(2);
        let client_order_id = ClientOrderId::unique_id();

        let reserve_parameters = test_object
            .balance_manager_base
            .create_reserve_parameters(Some(OrderSide::Buy), price, amount)
            .clone();

        let mut reservation_id = ReservationId::default();
        assert!(test_object.balance_manager_mut().try_reserve(
            &reserve_parameters,
            &mut reservation_id,
            &mut None,
        ));

        test_object.balance_manager_mut().approve_reservation(
            reservation_id,
            &client_order_id,
            approved_amount,
        );

        balance_map.insert(BalanceManagerBase::btc(), dec!(1.5));
        BalanceManagerBase::update_balance(
            test_object.balance_manager_mut(),
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
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::eth()
            ),
            Some(dec!(0.5))
        );
        assert_eq!(
            test_object.balance_manager().get_exchange_balance(
                exchange_account_id,
                test_object.balance_manager_base.currency_pair_metadata(),
                &BalanceManagerBase::bnb()
            ),
            Some(dec!(0.1))
        );
    }

    #[test]
    pub fn order_was_filled_last_fill_by_default_buy() {
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
            .create_order(OrderSide::Buy, ReservationId::default());

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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object
            .balance_manager_mut()
            .order_was_filled(&configuration_descriptor, &order, None);

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

    #[test]
    pub fn order_was_filled_last_fill_by_default_sell() {
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
            .create_order(OrderSide::Sell, ReservationId::default());

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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object
            .balance_manager_mut()
            .order_was_filled(&configuration_descriptor, &order, None);

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

    #[test]
    pub fn order_was_filled_specific_fill_buy() {
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
            .create_order(OrderSide::Buy, ReservationId::default());

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
        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            &configuration_descriptor,
            &order,
            order.fills.fills.first().cloned(),
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

    #[test]
    pub fn order_was_filled_specific_fill_sell() {
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
            .create_order(OrderSide::Sell, ReservationId::default());

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
        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object.balance_manager_mut().order_was_filled(
            &configuration_descriptor,
            &order,
            order.fills.fills.first().cloned(),
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

    #[test]
    pub fn order_was_finished_buy() {
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
            .create_order(OrderSide::Buy, ReservationId::default());

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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object
            .balance_manager_mut()
            .order_was_finished(&configuration_descriptor, &order)
            .expect("in test");

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

    #[test]
    pub fn order_was_finished_sell() {
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
            .create_order(OrderSide::Sell, ReservationId::default());
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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object
            .balance_manager_mut()
            .order_was_finished(&configuration_descriptor, &order)
            .expect("in test");

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

    #[test]
    pub fn order_was_finished_buy_sell() {
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
            .create_order(OrderSide::Buy, ReservationId::default());
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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object
            .balance_manager_mut()
            .order_was_finished(&configuration_descriptor, &order)
            .expect("in test");

        let mut order = test_object
            .balance_manager_base
            .create_order(OrderSide::Sell, ReservationId::default());
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

        let configuration_descriptor = test_object
            .balance_manager_base
            .configuration_descriptor
            .clone();
        test_object
            .balance_manager_mut()
            .order_was_finished(&configuration_descriptor, &order)
            .expect("in test");

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
}
