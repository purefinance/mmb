use crate::core::exchanges::events::AllowedEventSourceType;

// TODO: move to get_balance
pub enum BalancePositionOption {
    NonDerivative,
    SingleRequest,
    IndividualRequest,
}

#[derive(Debug)]
pub enum OpenOrdersType {
    None,
    AllCurrencyPair,
    // Some exchanges does not allow to get all open orders
    // So we should extract orders for each currency pair
    OneCurrencyPair,
}

pub struct ExchangeFeatures {
    pub open_orders_type: OpenOrdersType,
    pub empty_response_is_ok: bool,
    pub allows_to_get_order_info_by_client_order_id: bool,
    pub allowed_fill_event_source_type: AllowedEventSourceType,
    pub allowed_cancel_event_source_type: AllowedEventSourceType,
    pub balance_position_option: BalancePositionOption,
}

impl ExchangeFeatures {
    pub fn new(
        open_orders_type: OpenOrdersType,
        empty_response_is_ok: bool,
        allows_to_get_order_info_by_client_order_id: bool,
        allowed_fill_event_source_type: AllowedEventSourceType,
        allowed_cancel_event_source_type: AllowedEventSourceType,
    ) -> Self {
        Self {
            open_orders_type,
            empty_response_is_ok,
            allows_to_get_order_info_by_client_order_id,
            allowed_fill_event_source_type,
            allowed_cancel_event_source_type,
            balance_position_option: BalancePositionOption::NonDerivative,
        }
    }
}
