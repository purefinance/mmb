use std::collections::HashMap;

use crate::core::balance_manager::balance_position_by_fill_amount::BalancePositionByFillAmount;
use crate::core::balances::{
    balance_reservation_storage::BalanceReservationStorage,
    virtual_balance_holder::VirtualBalanceHolder,
};
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
