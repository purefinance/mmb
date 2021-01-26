use crate::core::order_book::order_book_data::OrderBookData;
use crate::core::DateTime;
use rust_decimal::prelude::*;
use std::collections::BTreeMap;

type Price = Decimal;
type Amount = Decimal;
type SortedOrderData = BTreeMap<Price, Amount>;

#[derive(Clone)]
pub struct LocalOrderBookSnapshot {
    pub asks: SortedOrderData,
    pub bids: SortedOrderData,
    pub last_update_time: DateTime,
}

impl LocalOrderBookSnapshot {
    pub fn new(asks: SortedOrderData, bids: SortedOrderData, last_update_time: DateTime) -> Self {
        Self {
            asks,
            bids,
            last_update_time,
        }
    }

    pub fn apply_update(&mut self, order_book_data: OrderBookData, update_time: DateTime) {
        Self::apply_update_by_side(order_book_data.asks, &mut self.asks);
        Self::apply_update_by_side(order_book_data.bids, &mut self.bids);
        self.last_update_time = update_time;
    }

    fn apply_update_by_side(updates: SortedOrderData, current_value: &mut SortedOrderData) {
        for (key, value) in updates.iter() {
            if value.is_zero() {
                let _ = current_value.remove(key);
            } else {
                let _ = current_value.insert(*key, *value);
            }
        }
    }
}
