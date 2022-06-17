use std::collections::HashMap;

use itertools::Itertools;
use mmb_utils::infrastructure::WithExpect;

use crate::balance::manager::balance_reservation::BalanceReservation;
use crate::orders::order::ReservationId;
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

    pub fn get(&self, reservation_id: ReservationId) -> Option<&BalanceReservation> {
        self.reserved_balances_by_id.get(&reservation_id)
    }

    pub fn get_mut(&mut self, reservation_id: ReservationId) -> Option<&mut BalanceReservation> {
        self.reserved_balances_by_id.get_mut(&reservation_id)
    }

    pub fn get_expected(&self, reservation_id: ReservationId) -> &BalanceReservation {
        self.get(reservation_id).with_expect(|| {
            format!(
                "Failed to get balance reservation with id = {}",
                reservation_id
            )
        })
    }

    pub fn get_mut_expected(&mut self, reservation_id: ReservationId) -> &mut BalanceReservation {
        self.get_mut(reservation_id).with_expect(|| {
            format!(
                "Failed to get mut balance reservation with id = {}",
                reservation_id
            )
        })
    }

    #[allow(clippy::needless_return)]
    fn update_metrics(&self) {
        if self.is_call_from_clone {
            // metrics should be saved only for original storage
            return;
        }

        //TODO: should be implemented
    }
}
