use crate::core::*;

use super::exchanges::common::*;
use crate::core::local_order_book_snapshot::LocalOrderBookSnapshot;
use std::collections::HashMap;

pub struct LocalSnapshotsService {
    local_snapshots: HashMap<ExchangeNameSymbol, LocalOrderBookSnapshot>,
}

impl LocalSnapshotsService {
    pub fn new(local_snapshots: HashMap<ExchangeNameSymbol, LocalOrderBookSnapshot>) -> Self {
        Self { local_snapshots }
    }

    pub fn get_snapshot(&self, snaphot_id: ExchangeNameSymbol) -> Option<&LocalOrderBookSnapshot> {
        self.local_snapshots.get(&snaphot_id)
    }

    pub fn update(
        &mut self,
        order_book_event: order_book_event::OrderBookEvent,
    ) -> Option<ExchangeIdSymbol> {
        // Extract all field
        let (
            _,
            creation_time,
            exchange_id,
            exchange_name,
            currency_code_pair,
            _,
            event_type,
            event_data,
        ) = order_book_event.dissolve();

        let exchange_symbol = ExchangeIdSymbol::new(exchange_id, exchange_name, currency_code_pair);

        match event_type {
            order_book_event::EventType::Snapshot => {
                let local_order_book_snapshot = event_data.to_local_order_book_snapshot();
                let exchanger_currency_state = exchange_symbol.get_exchanger_currency_state();

                self.local_snapshots
                    .insert(exchanger_currency_state.clone(), local_order_book_snapshot);

                return Some(exchange_symbol);
            }
            order_book_event::EventType::Update => {
                // Exctract variable here to avoid partial moving
                self.local_snapshots
                    .get_mut(&exchange_symbol.get_exchanger_currency_state())
                    .map(|snapshot| {
                        snapshot.apply_update(event_data, creation_time);
                        exchange_symbol
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use order_book_data::OrderDataMap;
    use rust_decimal_macros::*;

    #[test]
    fn update_by_full_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let mut asks = OrderDataMap::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = OrderDataMap::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));

        // Construct update
        let order_book_event = order_book_event::OrderBookEvent::new_for_update_tests(
            "does_not_matter".into(),
            CurrencyCodePair::new("does_not_matter".into()),
            order_book_event::EventType::Snapshot,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let exchange_id_symbol = snapshot_controller.update(order_book_event).unwrap();

        let updated_asks = &snapshot_controller
            .get_snapshot(exchange_id_symbol.get_exchanger_currency_state())
            .unwrap()
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(exchange_id_symbol.get_exchanger_currency_state())
            .unwrap()
            .bids;

        // Check all snapshot returned values
        assert_eq!(updated_asks.get(&dec!(1.0)), Some(&dec!(2.1)));
        assert_eq!(updated_asks.get(&dec!(3.0)), Some(&dec!(4.2)));
        assert_eq!(updated_bids.get(&dec!(2.9)), Some(&dec!(7.8)));
        assert_eq!(updated_bids.get(&dec!(3.4)), Some(&dec!(1.2)));
    }

    #[test]
    fn update_if_no_such_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let mut asks = OrderDataMap::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = OrderDataMap::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));

        // Construct update
        let order_book_event = order_book_event::OrderBookEvent::new_for_update_tests(
            "does_not_matter".into(),
            CurrencyCodePair::new("does_not_matter".into()),
            order_book_event::EventType::Update,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let update_result = snapshot_controller.update(order_book_event);

        // There was nothing to update
        assert!(update_result.is_none());
    }

    #[test]
    fn successful_update() {
        let test_exchange_name = "exchange_name";
        let test_currency_code_pair = "test_currency_code_pait";
        // Construct main object
        let exchange_name_symbol = ExchangeNameSymbol::new(
            test_exchange_name.into(),
            CurrencyCodePair::new(test_currency_code_pair.into()),
        );

        let mut primary_asks = OrderDataMap::new();
        primary_asks.insert(dec!(1.0), dec!(0.1));
        primary_asks.insert(dec!(3.0), dec!(4.2));
        let mut primary_bids = OrderDataMap::new();
        primary_bids.insert(dec!(2.9), dec!(7.8));
        primary_bids.insert(dec!(3.4), dec!(1.2));

        let primary_order_book_snapshot =
            LocalOrderBookSnapshot::new(primary_asks, primary_bids, Utc::now());

        let mut local_snapshots = HashMap::new();
        local_snapshots.insert(exchange_name_symbol, primary_order_book_snapshot);

        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let mut asks = OrderDataMap::new();
        asks.insert(dec!(1.0), dec!(2.1));
        let mut bids = OrderDataMap::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(0));

        // Construct update
        let order_book_event = order_book_event::OrderBookEvent::new_for_update_tests(
            test_exchange_name.into(),
            CurrencyCodePair::new(test_currency_code_pair.into()),
            order_book_event::EventType::Update,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let exchange_id_symbol = snapshot_controller.update(order_book_event).unwrap();

        let updated_asks = &snapshot_controller
            .get_snapshot(exchange_id_symbol.get_exchanger_currency_state())
            .unwrap()
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(exchange_id_symbol.get_exchanger_currency_state())
            .unwrap()
            .bids;

        // Check all snapshot returned values
        assert_eq!(
            updated_asks.get(&dec!(1.0)),
            // Updated
            Some(&dec!(2.1))
        );
        assert_eq!(
            updated_asks.get(&dec!(3.0)),
            // Not updated
            Some(&dec!(4.2))
        );
        assert_eq!(
            updated_bids.get(&dec!(2.9)),
            // Updated
            Some(&dec!(7.8))
        );
        assert_eq!(
            updated_bids.get(&dec!(3.4)),
            // Deleted
            None
        );
    }
}
