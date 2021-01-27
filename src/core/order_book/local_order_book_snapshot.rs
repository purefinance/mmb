use crate::core::exchanges::common::*;
use crate::core::order_book::order_book_data::OrderBookData;
use crate::core::DateTime;
use rust_decimal::prelude::*;
use std::collections::BTreeMap;

type SortedOrderData = BTreeMap<Price, Amount>;

pub enum OrderSide {
    Buy,
    Sell,
}

pub struct Order {
    price: Price,
    amount: Amount,
    side: OrderSide,
}

impl Order {
    pub fn new(price: Price, amount: Amount, side: OrderSide) -> Self {
        Self {
            price,
            amount,
            side,
        }
    }
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

    pub fn exclude_my_orders<T>(&mut self, orders: T)
    where
        T: IntoIterator<Item = Order>,
    {
        for price_level in orders.into_iter() {
            self.try_remove_order(price_level);
        }
    }

    fn try_remove_order(&mut self, order: Order) {
        let book_side = self.get_order_book_side(order.side);

        if let Some(amount) = book_side.get(&order.price) {
            let new_amount = amount - order.amount;

            if new_amount.is_sign_negative() || new_amount.is_zero() {
                let _ = book_side.remove(&order.price);
            } else {
                let _ = book_side.insert(order.price, new_amount);
            }
        }
    }

    fn get_order_book_side(&mut self, side: OrderSide) -> &mut SortedOrderData {
        match side {
            OrderSide::Buy => &mut self.bids,
            OrderSide::Sell => &mut self.asks,
        }
    }

    pub fn get_top_ask(&self) -> Option<(Price, Amount)> {
        if self.asks.is_empty() {
            return None;
        }

        // Get the first item (minimal)
        self.asks
            .iter()
            .next()
            .map(|price_level| (price_level.0.clone(), price_level.1.clone()))
    }

    pub fn get_top_bid(&self) -> Option<(Price, Amount)> {
        if self.bids.is_empty() {
            return None;
        }

        // Get the last item (maximum)
        self.bids
            .iter()
            .rev()
            .next()
            .map(|price_level| (price_level.0.clone(), price_level.1.clone()))
    }

    pub fn get_top(&self, book_side: OrderSide) -> Option<(Price, Amount)> {
        match book_side {
            OrderSide::Buy => self.get_top_bid(),
            OrderSide::Sell => self.get_top_bid(),
        }
    }

    pub fn get_asks_price_levels(&self) -> impl Iterator<Item = (&Price, &Amount)> {
        self.asks.iter()
    }

    pub fn get_bids_price_levels(&self) -> impl Iterator<Item = (&Price, &Amount)> {
        self.bids.iter().rev()
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
    fn get_asks_price_levels() {
        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(0.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let bids = SortedOrderData::new();

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let mut iter = order_book_snapshot.get_asks_price_levels();

        assert_eq!(iter.next().unwrap(), (&dec!(1.0), &dec!(0.1)));
        assert_eq!(iter.next().unwrap(), (&dec!(3.0), &dec!(4.2)));
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
    fn get_bids_price_levels() {
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.1));
        bids.insert(dec!(3.0), dec!(4.2));

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let mut iter = order_book_snapshot.get_bids_price_levels();

        assert_eq!(iter.next().unwrap(), (&dec!(3.0), &dec!(4.2)));
        assert_eq!(iter.next().unwrap(), (&dec!(1.0), &dec!(0.1)));
    }

    #[test]
    fn get_empty() {
        let asks = SortedOrderData::new();
        let bids = SortedOrderData::new();

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let top_bid = order_book_snapshot.get_top_ask();

        assert_eq!(top_bid, None);
    }

    #[test]
    fn remove_bid_order_completely() {
        // Construct update
        let order = Order::new(dec!(1.0), dec!(0.5), OrderSide::Buy);
        let orders = vec![order];

        // Construct main object
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.5));
        bids.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_my_orders(orders);

        let mut bids = order_book_snapshot.get_bids_price_levels();
        // Still exists
        assert_eq!(bids.next().unwrap(), (&dec!(3.0), &dec!(4.2)));
        // Was removed cause amount became <= 0
        assert_eq!(bids.next(), None);
    }

    #[test]
    fn decrease_bid_amount() {
        // Construct update
        let order = Order::new(dec!(1.0), dec!(0.3), OrderSide::Buy);
        let orders = vec![order];

        // Construct main object
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.5));
        bids.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_my_orders(orders);

        let mut bids = order_book_snapshot.get_bids_price_levels();
        // Still exists
        assert_eq!(bids.next().unwrap(), (&dec!(3.0), &dec!(4.2)));
        // Amount value was updated
        assert_eq!(bids.next().unwrap(), (&dec!(1.0), &dec!(0.2)));
    }

    #[test]
    fn remove_ask_order_completely() {
        // Construct update
        let order = Order::new(dec!(1.0), dec!(0.5), OrderSide::Sell);
        let orders = vec![order];

        // Construct main object
        let mut asks = SortedOrderData::new();
        let bids = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(0.5));
        asks.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_my_orders(orders);

        let mut asks = order_book_snapshot.get_asks_price_levels();
        // Still exists
        assert_eq!(asks.next().unwrap(), (&dec!(3.0), &dec!(4.2)));
        // Was removed cause amount became <= 0
        assert_eq!(asks.next(), None);
    }

    #[test]
    fn decrease_ask_amount() {
        // Construct update
        let order = Order::new(dec!(1.0), dec!(1.5), OrderSide::Sell);
        let orders = vec![order];

        // Construct main object
        let mut asks = SortedOrderData::new();
        let bids = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_my_orders(orders);

        let mut asks = order_book_snapshot.get_asks_price_levels();
        // Amount value was updated
        assert_eq!(asks.next().unwrap(), (&dec!(1.0), &dec!(0.6)));
        // Still exists
        assert_eq!(asks.next().unwrap(), (&dec!(3.0), &dec!(4.2)));
    }
}
