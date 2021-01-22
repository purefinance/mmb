use crate::core::local_order_book_snapshot::LocalOrderBookSnapshot;
use chrono::Utc;
use rust_decimal::*;
use std::collections::BTreeMap;

pub type OrderDataMap = BTreeMap<Decimal, Decimal>;

#[derive(Clone)]
pub struct OrderBookData {
    pub asks: OrderDataMap,
    pub bids: OrderDataMap,
}

impl OrderBookData {
    pub fn new_raw() -> Self {
        Self {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
        }
    }
    pub fn new(asks: OrderDataMap, bids: OrderDataMap) -> Self {
        Self { asks, bids }
    }

    pub fn to_local_order_book_snapshot(self) -> LocalOrderBookSnapshot {
        LocalOrderBookSnapshot::new(self.asks, self.bids, Utc::now())
    }

    // Сделать просто Vec вторым параметром
    pub fn update(&mut self, updates: Vec<OrderBookData>) {
        // If exists at least one update
        if updates.is_empty() {
            return;
        }

        self.update_inner_data(updates);
    }

    fn update_inner_data(&mut self, updates: Vec<OrderBookData>) {
        for update in updates.iter() {
            for (key, amount) in update.bids.iter() {
                self.bids.insert(*key, *amount);
            }

            for (key, amount) in update.asks.iter() {
                self.asks.insert(*key, *amount);
            }

            // TODO remove elements where value == 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_asks() {
        // Prepare data for updates
        let mut update_asks = OrderDataMap::new();
        update_asks.insert(Decimal::new(1, 0), Decimal::new(2, 0));
        update_asks.insert(Decimal::new(3, 0), Decimal::new(4, 0));

        let update_bids = OrderDataMap::new();

        // Create updates
        let update = OrderBookData::new(update_asks, update_bids);

        let updates = vec![update];

        // Prepare updated object
        let mut primary_asks = OrderDataMap::new();
        let primary_bids = OrderDataMap::new();
        primary_asks.insert(Decimal::new(1, 0), Decimal::new(1, 0));
        primary_asks.insert(Decimal::new(3, 0), Decimal::new(1, 0));

        let mut main_order_data = OrderBookData::new(primary_asks, primary_bids);

        main_order_data.update(updates);

        assert_eq!(
            main_order_data.asks.get(&Decimal::new(3, 0)),
            Some(&Decimal::new(4, 0))
        );
    }

    #[test]
    fn bids_update() {
        // Prepare data for updates
        let update_asks = OrderDataMap::new();

        let mut update_bids = OrderDataMap::new();
        update_bids.insert(Decimal::new(1, 0), Decimal::new(2, 2));
        update_bids.insert(Decimal::new(3, 0), Decimal::new(4, 0));

        // Create updates
        let update = OrderBookData::new(update_asks, update_bids);

        let updates = vec![update];

        // Prepare updated object
        let primary_asks = OrderDataMap::new();
        let mut primary_bids = OrderDataMap::new();
        primary_bids.insert(Decimal::new(1, 0), Decimal::new(1, 0));
        primary_bids.insert(Decimal::new(3, 0), Decimal::new(1, 0));

        let mut main_order_data = OrderBookData::new(primary_asks, primary_bids);

        main_order_data.update(updates);

        assert_eq!(
            main_order_data.bids.get(&Decimal::new(1, 0)),
            Some(&Decimal::new(2, 2))
        );
    }

    #[test]
    fn empty_update() {}

    #[test]
    fn several_updates() {}
}
