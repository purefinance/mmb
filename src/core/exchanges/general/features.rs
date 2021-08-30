use crate::core::exchanges::events::AllowedEventSourceType;

#[derive(Debug)]
pub enum OpenOrdersType {
    None,
    AllCurrencyPair,
    // Some exchanges does not allow to get all open orders
    // So we should extract orders for each currency pair
    OneCurrencyPair,
}

pub enum RestFillsType {
    None,
    OrderTrades,
    MyTrades,
    GetOrderInfo,
}

pub struct RestFillsFeatures {
    pub fills_type: RestFillsType,
    // TODO all over fields
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
