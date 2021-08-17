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

use rust_decimal::Decimal;
pub(crate) struct Balances {
    pub current_version: usize,
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

    /// Just for serialization/deserialization
    pub serialized_last_order_fills: Vec<(TradePlaceAccount, OrderFill)>,
}
