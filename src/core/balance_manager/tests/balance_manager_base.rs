#[cfg(test)]
use std::{collections::HashMap, sync::Arc};

#[double]
use crate::core::misc::time_manager::time_manager;
use crate::core::{
    balance_manager::{balance_manager::BalanceManager, balance_request::BalanceRequest},
    exchanges::{common::Price, events::ExchangeBalance},
    exchanges::{
        common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId},
        events::ExchangeBalancesAndPositions,
        general::currency_pair_metadata::CurrencyPairMetadata,
    },
    misc::{derivative_position::DerivativePosition, reserve_parameters::ReserveParameters},
    orders::order::{
        ClientOrderId, OrderExecutionType, OrderHeader, OrderSide, OrderSimpleProps, OrderSnapshot,
        OrderType, ReservationId,
    },
    service_configuration::configuration_descriptor::ConfigurationDescriptor,
};

use chrono::TimeZone;
use itertools::Itertools;
use mockall_double::double;
use parking_lot::{Mutex, MutexGuard};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub struct BalanceManagerBase {
    pub ten_digit_precision: Decimal,
    pub order_index: i32,
    pub exchange_account_id_1: ExchangeAccountId,
    pub exchange_account_id_2: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub configuration_descriptor: Arc<ConfigurationDescriptor>,
    pub balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    pub seconds_offset_in_mock: Arc<Mutex<u32>>,
    currency_pair_metadata: Option<Arc<CurrencyPairMetadata>>,

    mock_object: time_manager::__now::Context,
    mock_locker: MutexGuard<'static, ()>,
}

impl BalanceManagerBase {
    pub fn exchange_id() -> String {
        "local_exchange_id".into()
    }
    // Quote currency
    pub fn btc() -> CurrencyCode {
        "BTC".into()
    }
    // Base currency
    pub fn eth() -> CurrencyCode {
        "ETH".into()
    }
    // Another currency
    pub fn bnb() -> CurrencyCode {
        "BNB".into()
    }

    pub fn currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes(Self::eth(), Self::btc())
    }

    pub fn update_balance(
        mut balance_manager: MutexGuard<BalanceManager>,
        exchange_account_id: ExchangeAccountId,
        balances_by_currency_code: HashMap<CurrencyCode, Amount>,
    ) {
        balance_manager
            .update_exchange_balance(
                exchange_account_id,
                &ExchangeBalancesAndPositions {
                    balances: balances_by_currency_code
                        .iter()
                        .map(|x| ExchangeBalance {
                            currency_code: x.0.clone(),
                            balance: x.1.clone(),
                        })
                        .collect(),
                    positions: None,
                },
            )
            .expect("failed to update exchange balance");
    }

    pub fn update_balance_with_positions(
        mut balance_manager: MutexGuard<BalanceManager>,
        exchange_account_id: ExchangeAccountId,
        balances_by_currency_code: HashMap<CurrencyCode, Amount>,
        positions_by_currency_pair: HashMap<CurrencyPair, Decimal>,
    ) {
        let balances = balances_by_currency_code
            .into_iter()
            .map(|x| ExchangeBalance {
                currency_code: x.0,
                balance: x.1,
            })
            .collect_vec();

        let positions = Some(
            positions_by_currency_pair
                .into_iter()
                .map(|x| DerivativePositionInfo::new(x.0, x.1, None, dec!(0), dec!(0), dec!(1)))
                .collect_vec(),
        );

        balance_manager
            .update_exchange_balance(
                exchange_account_id,
                &ExchangeBalancesAndPositions {
                    balances,
                    positions,
                },
            )
            .expect("failed to update exchange balance");
    }

    pub fn new() -> Self {
        let seconds_offset_in_mock = Arc::new(Mutex::new(0u32));
        let (mock_object, mock_locker) =
            crate::core::misc::time_manager::tests::init_mock(seconds_offset_in_mock.clone());

        let exchange_id = Self::exchange_id().as_str().into();
        let exchange_account_id_1 = ExchangeAccountId::new(exchange_id, 0);
        let exchange_account_id_2 = ExchangeAccountId::new(exchange_id, 1);

        Self {
            ten_digit_precision: dec!(0.0000000001),
            order_index: 1,
            exchange_account_id_1,
            exchange_account_id_2,
            currency_pair: Self::currency_pair(),
            configuration_descriptor: Arc::from(ConfigurationDescriptor::new(
                "LiquidityGenerator".into(),
                exchange_account_id_1.to_string() + ";" + Self::currency_pair().as_str(),
            )),
            seconds_offset_in_mock,
            currency_pair_metadata: None,
            balance_manager: None,
            mock_object,
            mock_locker,
        }
    }
}

impl BalanceManagerBase {
    pub fn currency_pair_metadata(&self) -> Arc<CurrencyPairMetadata> {
        match &self.currency_pair_metadata {
            Some(res) => res.clone(),
            None => panic!("should be non None here"),
        }
    }

    pub fn balance_manager(&self) -> MutexGuard<BalanceManager> {
        match &self.balance_manager {
            Some(res) => res.lock(),
            None => panic!("should be non None here"),
        }
    }

    pub fn set_balance_manager(&mut self, input: Arc<Mutex<BalanceManager>>) {
        self.balance_manager = Some(input);
    }

    pub fn set_currency_pair_metadata(&mut self, input: Arc<CurrencyPairMetadata>) {
        self.currency_pair_metadata = Some(input);
    }

    pub fn create_balance_request(&self, currency_code: CurrencyCode) -> BalanceRequest {
        BalanceRequest::new(
            self.configuration_descriptor.clone(),
            self.exchange_account_id_1,
            self.currency_pair,
            currency_code,
        )
    }

    pub fn create_reserve_parameters(
        &self,
        order_side: OrderSide,
        price: Price,
        amount: Amount,
    ) -> ReserveParameters {
        ReserveParameters::new(
            self.configuration_descriptor.clone(),
            self.exchange_account_id_1,
            self.currency_pair_metadata(),
            order_side,
            price,
            amount,
        )
    }

    pub fn get_balance_by_trade_side(&self, side: OrderSide, price: Price) -> Option<Amount> {
        self.balance_manager().get_balance_by_side(
            self.configuration_descriptor.clone(),
            self.exchange_account_id_1,
            self.currency_pair_metadata(),
            side,
            price,
        )
    }

    pub fn get_balance_by_currency_code(
        &self,
        currency_code: CurrencyCode,
        price: Price,
    ) -> Option<Amount> {
        self.balance_manager().get_balance_by_currency_code(
            self.configuration_descriptor.clone(),
            self.exchange_account_id_1,
            self.currency_pair_metadata(),
            currency_code,
            price,
        )
    }

    pub fn get_balance_by_another_balance_manager_and_currency_code(
        &self,
        balance_manager: &BalanceManager,
        currency_code: CurrencyCode,
        price: Price,
    ) -> Option<Amount> {
        balance_manager.get_balance_by_currency_code(
            self.configuration_descriptor.clone(),
            self.exchange_account_id_1,
            self.currency_pair_metadata(),
            currency_code,
            price,
        )
    }

    pub fn create_order(
        &mut self,
        order_side: OrderSide,
        reservation_id: ReservationId,
    ) -> OrderSnapshot {
        self.create_order_by_amount(order_side, dec!(5), reservation_id)
    }

    pub fn create_order_by_amount(
        &mut self,
        order_side: OrderSide,
        amount: Amount,
        reservation_id: ReservationId,
    ) -> OrderSnapshot {
        let order_snapshot = OrderSnapshot {
            header: OrderHeader::new(
                ClientOrderId::new(format!("order{}", self.order_index).into()),
                time_manager::now(),
                self.exchange_account_id_1,
                self.currency_pair_metadata().currency_pair(),
                OrderType::Limit,
                order_side,
                amount,
                OrderExecutionType::None,
                Some(reservation_id),
                None,
                "balance_manager_base".into(),
            ),
            props: OrderSimpleProps::from_price(Some(dec!(0.2))),
            fills: Default::default(),
            status_history: Default::default(),
            internal_props: Default::default(),
        };
        self.order_index += 1;
        order_snapshot
    }
}
