use std::collections::HashMap;

use itertools::Itertools;

use crate::core::balance_manager::balance_reservation::BalanceReservation;
pub(crate) struct BalanceReservationStorage {
    reserved_balances_by_id: HashMap<i64, BalanceReservation>,
    pub is_call_from_me: bool,
}

impl BalanceReservationStorage {
    pub fn clear(&mut self) {
        self.reserved_balances_by_id.clear();
        self.update_metrics();
    }

    pub fn add(&mut self, reservation_id: i64, reservation: BalanceReservation) {
        self.reserved_balances_by_id
            .insert(reservation_id, reservation);
    }

    pub fn remove(&mut self, reservation_id: i64) {
        self.reserved_balances_by_id.remove(&reservation_id);
        self.update_metrics();
    }

    pub fn get_all_raw_reservations(&self) -> &HashMap<i64, BalanceReservation> {
        &self.reserved_balances_by_id
    }

    pub fn get_reservation_ids(&self) -> Vec<&i64> {
        self.reserved_balances_by_id.keys().collect_vec()
    }

    pub fn try_get(&self, reservation_id: &i64) -> Option<&BalanceReservation> {
        self.reserved_balances_by_id.get(reservation_id)
    }

    pub fn try_get_mut(&mut self, reservation_id: &i64) -> Option<&mut BalanceReservation> {
        self.reserved_balances_by_id.get_mut(reservation_id)
    }

    fn update_metrics(&self) {
        //TODO: should be implemented
    }
}
