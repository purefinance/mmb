use crate::core::exchanges::common::*;
use crate::core::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use crate::core::order_book::*;
use std::collections::HashMap;

/// Main local snapshot controller.
/// Allows create, update and view existing snapshots
// Формирует актуальный снэпшот ордербука
pub struct LocalSnapshotsService {
    local_snapshots: HashMap<ExchangeIdCurrencyPair, LocalOrderBookSnapshot>,
}

impl LocalSnapshotsService {
    pub fn new(local_snapshots: HashMap<ExchangeIdCurrencyPair, LocalOrderBookSnapshot>) -> Self {
        Self { local_snapshots }
    }

    pub fn get_snapshot(
        &self,
        snapshot_id: ExchangeIdCurrencyPair,
    ) -> Option<&LocalOrderBookSnapshot> {
        self.local_snapshots.get(&snapshot_id)
    }

    /// Create snaphot if it does not exist
    /// Update snapshot if suitable data arrive
    pub fn update(
        &mut self,
        order_book_event: order_book_event::OrderBookEvent,
    ) -> Option<ExchangeIdCurrencyPair> {
        // Extract all field
        let (_, creation_time, exchange_id, currency_pair, _, event_type, event_data) =
            order_book_event.dissolve();

        let exchange_symbol = ExchangeIdCurrencyPair::new(exchange_id, currency_pair);

        match event_type {
            order_book_event::EventType::Snapshot => {
                let local_order_book_snapshot = event_data.to_local_order_book_snapshot();

                self.local_snapshots
                    .insert(exchange_symbol.clone(), local_order_book_snapshot);

                return Some(exchange_symbol);
            }
            order_book_event::EventType::Update => {
                // Exctract variable here to avoid partial moving
                self.local_snapshots
                    .get_mut(&exchange_symbol)
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
    use chrono::Utc;
    use rust_decimal_macros::*;

    fn create_order_book_event_for_tests(
        exchange_id: ExchangeId,
        currency_pair: CurrencyPair,
        event_type: order_book_event::EventType,
        order_book_data: order_book_data::OrderBookData,
    ) -> order_book_event::OrderBookEvent {
        order_book_event::OrderBookEvent::new(
            Utc::now(),
            ExchangeAccountId::new(exchange_id, 0),
            currency_pair,
            "".to_string(),
            event_type,
            order_book_data,
        )
    }

    #[test]
    fn update_by_full_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_currency_codes("base".into(), "quote".into()),
            order_book_event::EventType::Snapshot,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let exchange_id_symbol = snapshot_controller.update(order_book_event).unwrap();

        let updated_asks = &snapshot_controller
            .get_snapshot(exchange_id_symbol.clone())
            .unwrap()
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(exchange_id_symbol)
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

        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_currency_codes("base".into(), "quote".into()),
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
        let test_exchange_id = "exchange_id";
        let test_currency_pair = CurrencyPair::from_currency_codes("base".into(), "quote".into());
        // Construct main object
        let exchange_id_symbol = ExchangeIdCurrencyPair::new(
            ExchangeAccountId::new(test_exchange_id.into(), 0),
            test_currency_pair.clone(),
        );

        let mut primary_asks = SortedOrderData::new();
        primary_asks.insert(dec!(1.0), dec!(0.1));
        primary_asks.insert(dec!(3.0), dec!(4.2));
        let mut primary_bids = SortedOrderData::new();
        primary_bids.insert(dec!(2.9), dec!(7.8));
        primary_bids.insert(dec!(3.4), dec!(1.2));

        let primary_order_book_snapshot =
            LocalOrderBookSnapshot::new(primary_asks, primary_bids, Utc::now());

        let mut local_snapshots = HashMap::new();
        local_snapshots.insert(exchange_id_symbol, primary_order_book_snapshot);

        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(2.1));
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(0));

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            test_exchange_id.into(),
            test_currency_pair,
            order_book_event::EventType::Update,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let exchange_id_symbol = snapshot_controller.update(order_book_event).unwrap();

        let updated_asks = &snapshot_controller
            .get_snapshot(exchange_id_symbol.clone())
            .unwrap()
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(exchange_id_symbol.clone())
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
