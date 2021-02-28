#[derive(Debug)]
pub enum OpenOrdersType {
    // FIXME do we really need this?
    None,
    OneCurrencyPair,
    AllCurrencyPair,
}

pub struct ExchangeFeatures {
    pub open_orders_type: OpenOrdersType,
}

impl ExchangeFeatures {
    pub fn new(open_orders_type: OpenOrdersType) -> Self {
        Self { open_orders_type }
    }
}
