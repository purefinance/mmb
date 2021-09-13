use std::collections::HashMap;

use itertools::Itertools;

use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::orders::order::ReservationId;
#[derive(Clone)]
pub(crate) struct BalanceReservationStorage {
    reserved_balances_by_id: HashMap<ReservationId, BalanceReservation>,
    pub is_call_from_clone: bool,
}

impl BalanceReservationStorage {
    pub fn new() -> Self {
        Self {
            reserved_balances_by_id: HashMap::new(),
            is_call_from_clone: false,
        }
    }
    pub fn clear(&mut self) {
        self.reserved_balances_by_id.clear();
        self.update_metrics();
    }

    pub fn add(&mut self, reservation_id: ReservationId, reservation: BalanceReservation) {
        self.reserved_balances_by_id
            .insert(reservation_id, reservation);
        self.update_metrics();
    }

    pub fn remove(&mut self, reservation_id: ReservationId) {
        self.reserved_balances_by_id.remove(&reservation_id);
        self.update_metrics();
    }

    pub fn get_all_raw_reservations(&self) -> &HashMap<ReservationId, BalanceReservation> {
        &self.reserved_balances_by_id
    }

    pub fn get_reservation_ids(&self) -> Vec<ReservationId> {
        self.reserved_balances_by_id.keys().cloned().collect_vec()
    }

    pub fn try_get(&self, reservation_id: &ReservationId) -> Option<&BalanceReservation> {
        self.reserved_balances_by_id.get(reservation_id)
    }

    pub fn try_get_mut(
        &mut self,
        reservation_id: &ReservationId,
    ) -> Option<&mut BalanceReservation> {
        self.reserved_balances_by_id.get_mut(reservation_id)
    }

    fn update_metrics(&self) {
        if self.is_call_from_clone {
            // metrics should be saved only for original storage
            return;
        }

        //TODO: should be implemented
    }
}
