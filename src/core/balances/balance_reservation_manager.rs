use std::collections::HashMap;

use itertools::Itertools;
use rust_decimal::Decimal;

use crate::core::balance_manager::balance_position_by_fill_amount::BalancePositionByFillAmount;
use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::balances::{
    balance_reservation_storage::BalanceReservationStorage,
    virtual_balance_holder::VirtualBalanceHolder,
};
use crate::core::exchanges::general::currency_pair_metadata::BeforeAfter;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::misc::service_value_tree::ServiceValueTree;

pub(crate) struct BalanceReservationManager {
    exchanges_by_id: HashMap<String, Exchange>,

    // private readonly ICurrencyPairToSymbolConverter _currencyPairToSymbolConverter;
    // private readonly IDateTimeService _dateTimeService;
    // private readonly ILogger _logger = Log.ForContext<BalanceReservationManager>();
    reserved_amount_in_amount_currency: ServiceValueTree,
    amount_limits_in_amount_currency: ServiceValueTree,

    position_by_fill_amount_in_amount_currency: BalancePositionByFillAmount,
    reservation_id: i64, // Utils.GetCurrentMiliseconds();

    pub virtual_balance_holder: VirtualBalanceHolder,
    pub balance_reservation_storage: BalanceReservationStorage,

    is_call_from_clone: bool,
}

impl BalanceReservationManager {
    pub fn update_reserved_balances(
        &mut self,
        reserved_balances_by_id: HashMap<i64, BalanceReservation>,
    ) {
        self.balance_reservation_storage.clear();
        for (reservation_id, reservation) in reserved_balances_by_id {
            self.balance_reservation_storage
                .add(reservation_id, reservation);
        }
        self.sync_reservation_amounts();
    }

    pub fn sync_reservation_amounts(&mut self) {
        let reservations = self
            .balance_reservation_storage
            .get_all_raw_reservations()
            .values()
            .collect_vec();

        let mut reserved_by_request: HashMap<BalanceRequest, Decimal> = HashMap::new();
        for reservation in reservations {
            let balance_request = BalanceRequest::new(
                reservation.configuration_descriptor.clone(),
                reservation.exchange_account_id.clone(),
                reservation.currency_pair_metadata.currency_pair(),
                reservation
                    .currency_pair_metadata
                    .get_trade_code(reservation.order_side, BeforeAfter::Before),
            );
            if let Some(grouped_reservations) = reserved_by_request.get_mut(&balance_request) {
                *grouped_reservations += reservation.unreserved_amount;
            } else {
                reserved_by_request.insert(balance_request, reservation.unreserved_amount);
            }
        }

        let mut svt = ServiceValueTree::new();
        for (request, reserved) in reserved_by_request {
            svt.set_by_balance_request(&request, reserved);
        }
        self.reserved_amount_in_amount_currency = svt;
    }
}
