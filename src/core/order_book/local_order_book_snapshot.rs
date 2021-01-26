use crate::core::exchanges::common::*;
use crate::core::order_book::order_book_data::OrderBookData;
use crate::core::DateTime;
use rust_decimal::prelude::*;
use std::collections::BTreeMap;

type SortedOrderData = BTreeMap<Price, Amount>;

pub enum OrderSide {
    Unknown,
    Buy,
    Sell,
}

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

    pub fn get_top_ask(&self) -> Option<(Price, Amount)> {
        Self::get_top(&self.asks)
    }

    pub fn get_top_bid(&self) -> Option<(Price, Amount)> {
        if self.bids.is_empty() {
            return None;
        }

        // Get the first item (minimal)
        self.bids
            .iter()
            .rev()
            .next()
            .map(|price_level| (price_level.0.clone(), price_level.1.clone()))
    }

    fn get_top(book_side: &SortedOrderData) -> Option<(Price, Amount)> {
        if book_side.is_empty() {
            return None;
        }

        // Get the first item (minimal)
        book_side
            .iter()
            .next()
            .map(|price_level| (price_level.0.clone(), price_level.1.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::*;

    #[test]
    fn get_top_ask() {
        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(0.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let bids = SortedOrderData::new();

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let top_ask = order_book_snapshot.get_top_ask().unwrap();

        assert_eq!(top_ask, (dec!(1.0), dec!(0.1)))
    }

    #[test]
    fn get_top_bid() {
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.1));
        bids.insert(dec!(3.0), dec!(4.2));

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let top_bid = order_book_snapshot.get_top_bid().unwrap();

        assert_eq!(top_bid, (dec!(3.0), dec!(4.2)))
    }

    #[test]
    fn get_empty() {
        let asks = SortedOrderData::new();
        let bids = SortedOrderData::new();

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let top_bid = order_book_snapshot.get_top_ask();

        assert_eq!(top_bid, None);
    }
}
