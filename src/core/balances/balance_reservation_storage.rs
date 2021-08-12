use std::collections::HashMap;

use crate::core::balance_manager::balance_reservation::BalanceReservation;
pub(crate) struct BalanceReservationStorage {
    reserved_balances_by_id: HashMap<i64, BalanceReservation>,
}
