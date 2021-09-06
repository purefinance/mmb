use crate::core::exchanges::events::AllowedEventSourceType;

#[derive(Debug)]
pub enum OpenOrdersType {
    None,
    AllCurrencyPair,
    // Some exchanges does not allow to get all open orders
    // So we should extract orders for each currency pair
    OneCurrencyPair,
}

#[derive(Debug)]
pub enum RestFillsType {
    None,
    OrderTrades,
    MyTrades,
    GetOrderInfo,
}

impl Default for RestFillsType {
    fn default() -> Self {
        RestFillsType::None
    }
}

#[derive(Default, Debug)]
pub struct RestFillsFeatures {
    pub fills_type: RestFillsType,
    // TODO all over fields for check_order_fills()
}

impl RestFillsFeatures {
    pub fn new(fills_type: RestFillsType) -> Self {
        Self { fills_type }
    }
}

pub struct ExchangeFeatures {
    pub open_orders_type: OpenOrdersType,
    pub rest_fills_features: RestFillsFeatures,
    pub empty_response_is_ok: bool,
    pub allows_to_get_order_info_by_client_order_id: bool,
    pub allowed_fill_event_source_type: AllowedEventSourceType,
    pub allowed_cancel_event_source_type: AllowedEventSourceType,
}

impl ExchangeFeatures {
    pub fn new(
        open_orders_type: OpenOrdersType,
        rest_fills_features: RestFillsFeatures,
        empty_response_is_ok: bool,
        allows_to_get_order_info_by_client_order_id: bool,
        allowed_fill_event_source_type: AllowedEventSourceType,
        allowed_cancel_event_source_type: AllowedEventSourceType,
    ) -> Self {
        Self {
            open_orders_type,
            rest_fills_features,
            empty_response_is_ok,
            allows_to_get_order_info_by_client_order_id,
            allowed_fill_event_source_type,
            allowed_cancel_event_source_type,
        }
    }
}
