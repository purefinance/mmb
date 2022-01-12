use crate::exchanges::common::Price;

#[derive(Clone, PartialEq, Eq)]
pub struct PriceByOrderSide {
    pub top_bid: Option<Price>,
    pub top_ask: Option<Price>,
}

impl PriceByOrderSide {
    pub fn new(top_bid: Option<Price>, top_ask: Option<Price>) -> Self {
        Self { top_bid, top_ask }
    }
}
