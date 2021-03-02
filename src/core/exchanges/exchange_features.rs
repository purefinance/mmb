#[derive(Debug)]
pub enum OpenOrdersType {
    None,
    OneCurrencyPair,
    AllCurrencyPair,
}

pub struct ExchangeFeatures {
    pub open_orders_type: OpenOrdersType,
    pub empty_response_is_ok: bool,
}

impl ExchangeFeatures {
    pub fn new(open_orders_type: OpenOrdersType, empty_response_is_ok: bool) -> Self {
        Self {
            open_orders_type,
            empty_response_is_ok,
        }
    }
}
