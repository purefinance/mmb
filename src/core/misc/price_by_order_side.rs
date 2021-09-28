use crate::core::exchanges::common::Price;

pub struct PriceByOrderSide {
    pub top_bid: Option<Price>,
    pub top_ask: Option<Price>,
}

impl PriceByOrderSide {
    pub fn new(top_bid: Option<Price>, top_ask: Option<Price>) -> Self {
        Self { top_bid, top_ask }
    }
}
