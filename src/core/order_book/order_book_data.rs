use crate::core::exchanges::common::*;
use crate::core::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use chrono::Utc;

/// Main asks and bids storage
#[derive(Debug, Clone)]
pub struct OrderBookData {
    pub asks: SortedOrderData,
    pub bids: SortedOrderData,
}

impl OrderBookData {
    pub fn new(asks: SortedOrderData, bids: SortedOrderData) -> Self {
        Self { asks, bids }
    }

    pub fn to_local_order_book_snapshot(self) -> LocalOrderBookSnapshot {
        LocalOrderBookSnapshot::new(self.asks, self.bids, Utc::now())
    }

    /// Perform inner asks and bids update
    pub fn update(&mut self, updates: Vec<OrderBookData>) {
        if updates.is_empty() {
            return;
        }

        self.update_inner_data(updates);
    }

    fn update_inner_data(&mut self, updates: Vec<OrderBookData>) {
        for update in updates.into_iter() {
            Self::apply_update(&mut self.asks, &mut self.bids, update);
        }
    }

    pub(crate) fn apply_update(
        asks: &mut SortedOrderData,
        bids: &mut SortedOrderData,
        update: OrderBookData,
    ) {
        Self::update_by_side(asks, update.asks);
        Self::update_by_side(bids, update.bids);
    }

    fn update_by_side(snapshot: &mut SortedOrderData, update: SortedOrderData) {
        for (key, amount) in update.into_iter() {
            if amount.is_zero() {
                let _ = snapshot.remove(&key);
            } else {
                let _ = snapshot.insert(key, amount);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::*;

    #[test]
    fn update_asks() {
        // Prepare data for updates
        let mut update_asks = SortedOrderData::new();
        update_asks.insert(dec!(1.0), dec!(2.0));
        update_asks.insert(dec!(3.0), dec!(4.0));

        let update_bids = SortedOrderData::new();

        // Create updates
        let update = OrderBookData::new(update_asks, update_bids);

        let updates = vec![update];

        // Prepare updated object
        let mut primary_asks = SortedOrderData::new();
        let primary_bids = SortedOrderData::new();
        primary_asks.insert(dec!(1.0), dec!(1.0));
        primary_asks.insert(dec!(3.0), dec!(1.0));

        let mut main_order_data = OrderBookData::new(primary_asks, primary_bids);

        main_order_data.update(updates);

        assert_eq!(main_order_data.asks.get(&dec!(1.0)), Some(&dec!(2.0)));
        assert_eq!(main_order_data.asks.get(&dec!(3.0)), Some(&dec!(4.0)));
    }

    #[test]
    fn bids_update() {
        // Prepare data for updates
        let update_asks = SortedOrderData::new();

        let mut update_bids = SortedOrderData::new();
        update_bids.insert(dec!(1.0), dec!(2.2));
        update_bids.insert(dec!(3.0), dec!(4.0));

        // Create updates
        let update = OrderBookData::new(update_asks, update_bids);

        let updates = vec![update];

        // Prepare updated object
        let primary_asks = SortedOrderData::new();
        let mut primary_bids = SortedOrderData::new();
        primary_bids.insert(dec!(1.0), dec!(1.0));
        primary_bids.insert(dec!(3.0), dec!(1.0));

        let mut main_order_data = OrderBookData::new(primary_asks, primary_bids);

        main_order_data.update(updates);

        assert_eq!(main_order_data.bids.get(&dec!(1.0)), Some(&dec!(2.2)));
        assert_eq!(main_order_data.bids.get(&dec!(3.0)), Some(&dec!(4.0)));
    }

    #[test]
    fn empty_update() {
        // Prepare data for empty update
        let updates = Vec::new();

        // Prepare updated object
        let primary_asks = SortedOrderData::new();
        let mut primary_bids = SortedOrderData::new();
        primary_bids.insert(dec!(1.0), dec!(1.0));
        primary_bids.insert(dec!(3.0), dec!(1.0));

        let mut main_order_data = OrderBookData::new(primary_asks, primary_bids);

        main_order_data.update(updates);

        assert_eq!(main_order_data.bids.get(&dec!(1.0)), Some(&dec!(1.0)));
        assert_eq!(main_order_data.bids.get(&dec!(3.0)), Some(&dec!(1.0)));
    }

    #[test]
    fn several_updates() {
        // Prepare data for updates
        let mut first_update_asks = SortedOrderData::new();
        first_update_asks.insert(dec!(1.0), dec!(2.0));
        first_update_asks.insert(dec!(3.0), dec!(4.0));
        let first_update_bids = SortedOrderData::new();

        let mut second_update_asks = SortedOrderData::new();
        second_update_asks.insert(dec!(1.0), dec!(2.8));
        second_update_asks.insert(dec!(6.0), dec!(0));
        let second_update_bids = SortedOrderData::new();

        // Create updates
        let first_update = OrderBookData::new(first_update_asks, first_update_bids);
        let second_update = OrderBookData::new(second_update_asks, second_update_bids);

        let updates = vec![first_update, second_update];

        // Prepare updated object
        let mut primary_asks = SortedOrderData::new();
        let primary_bids = SortedOrderData::new();
        primary_asks.insert(dec!(1.0), dec!(1.0));
        primary_asks.insert(dec!(2.0), dec!(5.6));
        primary_asks.insert(dec!(3.0), dec!(1.0));
        primary_asks.insert(dec!(6.0), dec!(1.0));

        let mut main_order_data = OrderBookData::new(primary_asks, primary_bids);

        main_order_data.update(updates);

        // Updated from second update
        assert_eq!(main_order_data.asks.get(&dec!(1.0)), Some(&dec!(2.8)));
        // Unchanged
        assert_eq!(main_order_data.asks.get(&dec!(2.0)), Some(&dec!(5.6)));
        // Updated from first update
        assert_eq!(main_order_data.asks.get(&dec!(3.0)), Some(&dec!(4.0)));
        // Deleted because 0 amount in second update
        assert_eq!(main_order_data.asks.get(&dec!(6.0)), None);
    }
}
