use crate::DateTime;
use rust_decimal::prelude::*;
use std::collections::BTreeMap;

type SortedOrderData = BTreeMap<Decimal, Decimal>;
pub struct LocalOrderBookSnapshot {
    asks: SortedOrderData,
    bids: SortedOrderData,
    last_update_time: DateTime,
}

impl LocalOrderBookSnapshot {
    pub fn new(asks: SortedOrderData, bids: SortedOrderData, last_update_time: DateTime) -> Self {
        Self {
            asks,
            bids,
            last_update_time,
        }
    }
}
