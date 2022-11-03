use crate::market::*;
use crate::order::snapshot::*;
use crate::order::snapshot::{PriceByOrderSide, SortedOrderData};
use crate::order_book::order_book_data::OrderBookData;
use mmb_utils::DateTime;
use rust_decimal_macros::dec;

/// Fields from OrderSnapshot for exclude order
pub struct DataToExcludeOrder {
    price: Price,
    amount: Amount,
    side: OrderSide,
}

impl DataToExcludeOrder {
    pub fn new(price: Price, amount: Amount, side: OrderSide) -> Self {
        Self {
            price,
            amount,
            side,
        }
    }
}

pub enum ResultAskBidFix {
    Ok,
    Fixed {
        /// Original top ask price
        top_ask: Price,
        /// Original top bid price
        top_bid: Price,
    },
}

/// Snapshot of certain ask and bids collection
/// Identified by ExchangeId
#[derive(Clone, Debug)]
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

    /// Update inner asks and bids
    pub fn apply_update(&mut self, update: &OrderBookData, update_time: DateTime) {
        OrderBookData::apply_update(&mut self.asks, &mut self.bids, update);
        self.last_update_time = update_time;
    }

    pub fn exclude_orders<T>(&mut self, orders: T)
    where
        T: IntoIterator<Item = DataToExcludeOrder>,
    {
        for price_level in orders.into_iter() {
            self.try_remove_order(price_level);
        }
    }

    /// Return value with minimum price
    pub fn get_top_ask(&self) -> Option<(Price, Amount)> {
        // Get the first item (minimal)
        self.get_asks_price_levels()
            .next()
            .map(|(&price, &amount)| (price, amount))
    }

    /// Return value with maximum price
    pub fn get_top_bid(&self) -> Option<(Price, Amount)> {
        // Get the last item (maximum)
        self.get_bids_price_levels()
            .next()
            .map(|(&price, &amount)| (price, amount))
    }

    /// Return top value of asks or bids
    pub fn get_top(&self, book_side: OrderSide) -> Option<(Price, Amount)> {
        match book_side {
            OrderSide::Buy => self.get_top_bid(),
            OrderSide::Sell => self.get_top_ask(),
        }
    }

    /// Return all asks values starting from the lowest price
    pub fn get_asks_price_levels(&self) -> impl Iterator<Item = (&Price, &Amount)> {
        self.asks.iter()
    }

    /// Return all asks values starting from the highest price
    pub fn get_bids_price_levels(&self) -> impl Iterator<Item = (&Price, &Amount)> {
        self.bids.iter().rev()
    }

    fn try_remove_order(&mut self, order: DataToExcludeOrder) {
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

    pub fn get_top_prices(&self) -> PriceByOrderSide {
        let top_bid = self.get_top_bid().map(|(price, _)| price);
        let top_ask = self.get_top_ask().map(|(price, _)| price);

        PriceByOrderSide::new(top_bid, top_ask)
    }

    pub fn calculate_middle_price(&self, market_id: MarketId) -> Option<Price> {
        let prices = self.get_top_prices();
        let top_ask = match prices.top_ask {
            Some(top_ask) => top_ask,
            None => {
                log::warn!(
                "Can't get top ask price in {:?} in LocalOrderBookSnapshot::calculate_middle_price() {:?}",
                market_id,
                self
            );
                return None;
            }
        };

        let top_bid = match prices.top_bid {
            Some(top_bid) => top_bid,
            None => {
                log::warn!(
                "Can't get top bid price in {:?} in LocalOrderBookSnapshot::calculate_middle_price() {:?}",
                market_id,
                self
            );
                return None;
            }
        };

        Some((top_ask + top_bid) * dec!(0.5))
    }

    /// Removed asks and bids between top price levels if it's crossed
    pub fn fix_asks_bids_if_needed(&mut self) -> ResultAskBidFix {
        match self.get_top_prices() {
            PriceByOrderSide {
                top_ask: Some(top_ask),
                top_bid: Some(top_bid),
            } if top_ask <= top_bid => {
                self.fix_asks_bids(top_ask, top_bid);
                ResultAskBidFix::Fixed { top_ask, top_bid }
            }
            _ => ResultAskBidFix::Ok,
        }
    }

    fn fix_asks_bids(&mut self, top_ask: Price, top_bid: Price) {
        loop {
            if let Some((&bid_price, _)) = self.bids.iter().rev().next() {
                if bid_price >= top_ask {
                    self.bids.remove(&bid_price);
                    continue;
                }
            }

            break;
        }

        loop {
            if let Some((&ask_price, _)) = self.asks.iter().next() {
                if ask_price <= top_bid {
                    self.asks.remove(&ask_price);
                    continue;
                }
            }

            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn get_top_ask() {
        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(0.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let bids = SortedOrderData::new();

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let top_ask = order_book_snapshot.get_top_ask().expect("in test");

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

        assert_eq!(iter.next().expect("in test"), (&dec!(1.0), &dec!(0.1)));
        assert_eq!(iter.next().expect("in test"), (&dec!(3.0), &dec!(4.2)));
    }

    #[test]
    fn get_top_bid() {
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.1));
        bids.insert(dec!(3.0), dec!(4.2));

        let order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        let top_bid = order_book_snapshot.get_top_bid().expect("in test");

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

        assert_eq!(iter.next().expect("in test"), (&dec!(3.0), &dec!(4.2)));
        assert_eq!(iter.next().expect("in test"), (&dec!(1.0), &dec!(0.1)));
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
        let order = DataToExcludeOrder::new(dec!(1.0), dec!(0.5), OrderSide::Buy);
        let orders = vec![order];

        // Construct main object
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.5));
        bids.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_orders(orders);

        let mut bids = order_book_snapshot.get_bids_price_levels();
        // Still exists
        assert_eq!(bids.next().expect("in test"), (&dec!(3.0), &dec!(4.2)));
        // Was removed cause amount became <= 0
        assert_eq!(bids.next(), None);
    }

    #[test]
    fn decrease_bid_amount() {
        // Construct update
        let order = DataToExcludeOrder::new(dec!(1.0), dec!(0.3), OrderSide::Buy);
        let orders = vec![order];

        // Construct main object
        let asks = SortedOrderData::new();
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(1.0), dec!(0.5));
        bids.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_orders(orders);

        let mut bids = order_book_snapshot.get_bids_price_levels();
        // Still exists
        assert_eq!(bids.next().expect("in test"), (&dec!(3.0), &dec!(4.2)));
        // Amount value was updated
        assert_eq!(bids.next().expect("in test"), (&dec!(1.0), &dec!(0.2)));
    }

    #[test]
    fn remove_ask_order_completely() {
        // Construct update
        let order = DataToExcludeOrder::new(dec!(1.0), dec!(0.5), OrderSide::Sell);
        let orders = vec![order];

        // Construct main object
        let mut asks = SortedOrderData::new();
        let bids = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(0.5));
        asks.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_orders(orders);

        let mut asks = order_book_snapshot.get_asks_price_levels();
        // Still exists
        assert_eq!(asks.next().expect("in test"), (&dec!(3.0), &dec!(4.2)));
        // Was removed cause amount became <= 0
        assert_eq!(asks.next(), None);
    }

    #[test]
    fn decrease_ask_amount() {
        // Construct update
        let order = DataToExcludeOrder::new(dec!(1.0), dec!(1.5), OrderSide::Sell);
        let orders = vec![order];

        // Construct main object
        let mut asks = SortedOrderData::new();
        let bids = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));

        let mut order_book_snapshot = LocalOrderBookSnapshot::new(asks, bids, Utc::now());

        order_book_snapshot.exclude_orders(orders);

        let mut asks = order_book_snapshot.get_asks_price_levels();
        // Amount value was updated
        assert_eq!(asks.next().expect("in test"), (&dec!(1.0), &dec!(0.6)));
        // Still exists
        assert_eq!(asks.next().expect("in test"), (&dec!(3.0), &dec!(4.2)));
    }
}
