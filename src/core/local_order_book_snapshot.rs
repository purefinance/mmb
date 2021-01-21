use rust_decimal::prelude::*;
use std::collections::BTreeMap;

type SortedOrderData = BTreeMap<Decimal, Decimal>;
pub struct LocalOrderBookSnapshot {
    asks: SortedOrderData,
    bids: SortedOrderData,
}

impl LocalOrderBookSnapshot {
    pub fn new(asks: SortedOrderData, bids: SortedOrderData) -> Self {
        Self { asks, bids }
    }
}
