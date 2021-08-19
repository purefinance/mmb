use std::collections::HashMap;

use crate::core::balance_manager::{
    balance_position_by_fill_amount::BalancePositionByFillAmount,
    balance_reservation::BalanceReservation,
};
use crate::core::exchanges::common::CurrencyCode;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::common::TradePlaceAccount;
use crate::core::misc::service_value_tree::ServiceValueTree;
use crate::core::orders::fill::OrderFill;

use itertools::Itertools;
use rust_decimal::Decimal;

// TODO: add storage item like in C# if needed
pub(crate) struct Balances {
    pub version: usize,
    pub balances_by_exchange_id: Option<HashMap<ExchangeAccountId, HashMap<CurrencyCode, Decimal>>>,
    pub virtual_diff_balances: Option<ServiceValueTree>,

    /// In Amount currency
    pub reserved_amount: Option<ServiceValueTree>,

    /// In Amount currency
    pub position_by_fill_amount: Option<BalancePositionByFillAmount>,

    /// In Amount currency
    pub amount_limits: Option<ServiceValueTree>,
    pub balance_reservations_by_reservation_id: Option<HashMap<i64, BalanceReservation>>,

    pub last_order_fills: HashMap<TradePlaceAccount, OrderFill>,
}

impl Balances {
    pub fn new(
        balances_by_exchange_id: HashMap<ExchangeAccountId, HashMap<CurrencyCode, Decimal>>,
        virtual_diff_balances: ServiceValueTree,
        reserved_amount: ServiceValueTree,
        position_by_fill_amount: BalancePositionByFillAmount,
        amount_limits: ServiceValueTree,
        balance_reservations_by_reservation_id: HashMap<i64, BalanceReservation>,
        serialized_last_order_fills: Option<Vec<(TradePlaceAccount, OrderFill)>>,
    ) -> Self {
        let mut res = Self {
            version: Balances::get_curren_version(),
            balances_by_exchange_id: Some(balances_by_exchange_id),
            virtual_diff_balances: Some(virtual_diff_balances),
            reserved_amount: Some(reserved_amount),
            position_by_fill_amount: Some(position_by_fill_amount),
            amount_limits: Some(amount_limits),
            balance_reservations_by_reservation_id: Some(balance_reservations_by_reservation_id),
            last_order_fills: HashMap::new(),
        };
        res.set_serialized_last_order_fiils(serialized_last_order_fills);
        res
    }

    pub fn get_curren_version() -> usize {
        1
    }

    pub fn get_serialized_last_order_fills(&self) -> Option<Vec<(&TradePlaceAccount, &OrderFill)>> {
        Some(self.last_order_fills.iter().collect_vec())
    }

    pub fn set_serialized_last_order_fiils(
        &mut self,
        input: Option<Vec<(TradePlaceAccount, OrderFill)>>,
    ) {
        match input {
            Some(input) => {
                for (k, v) in input {
                    self.last_order_fills.insert(k, v);
                }
            }
            None => self.last_order_fills = HashMap::new(),
        }
    }
}
