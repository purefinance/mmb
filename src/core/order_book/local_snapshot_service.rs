use crate::core::exchanges::common::*;
use crate::core::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use crate::core::order_book::*;
use std::collections::HashMap;

/// Produce and actualize current logical state of order book snapshot according to logical time of handled order book events
pub struct LocalSnapshotsService {
    local_snapshots: HashMap<TradePlace, LocalOrderBookSnapshot>,
}

impl LocalSnapshotsService {
    pub fn new(local_snapshots: HashMap<TradePlace, LocalOrderBookSnapshot>) -> Self {
        Self { local_snapshots }
    }

    pub fn get_snapshot(&self, trade_place: TradePlace) -> Option<&LocalOrderBookSnapshot> {
        self.local_snapshots.get(&trade_place)
    }

    /// Create snapshot if it does not exist
    /// Update snapshot if suitable data arrive
    pub fn update(&mut self, order_book_event: event::OrderBookEvent) -> Option<TradePlaceAccount> {
        // Extract all field
        let (_, creation_time, exchange_account_id, currency_pair, _, event_type, event_data) =
            order_book_event.dissolve();

        let trade_place = TradePlace::new(
            exchange_account_id.exchange_id.clone(),
            currency_pair.clone(),
        );

        match event_type {
            event::EventType::Snapshot => {
                let _ = self.local_snapshots.insert(
                    trade_place.clone(),
                    event_data.to_local_order_book_snapshot(),
                );

                Some(TradePlaceAccount::new(exchange_account_id, currency_pair))
            }
            event::EventType::Update => {
                self.local_snapshots.get_mut(&trade_place).map(|snapshot| {
                    snapshot.apply_update(event_data, creation_time);
                    TradePlaceAccount::new(exchange_account_id, currency_pair)
                })
            }
        }
    }
}

impl Default for LocalSnapshotsService {
    fn default() -> Self {
        LocalSnapshotsService::new(HashMap::new())
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
        event_type: event::EventType,
        order_book_data: order_book_data::OrderBookData,
    ) -> event::OrderBookEvent {
        event::OrderBookEvent::new(
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
            CurrencyPair::from_codes(&"base".into(), &"quote".into()),
            event::EventType::Snapshot,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let trade_place_account = snapshot_controller
            .update(order_book_event)
            .expect("in test");

        let updated_asks = &snapshot_controller
            .get_snapshot(trade_place_account.trade_place())
            .expect("in test")
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(trade_place_account.trade_place())
            .expect("in test")
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
        let mut snapshot_service = LocalSnapshotsService::new(local_snapshots);

        let mut asks = SortedOrderData::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = SortedOrderData::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes(&"base".into(), &"quote".into()),
            event::EventType::Update,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let update_result = snapshot_service.update(order_book_event);

        // There was nothing to update
        assert!(update_result.is_none());
    }

    #[test]
    fn successful_update() {
        let test_exchange_id = "exchange_id";
        let test_currency_pair = CurrencyPair::from_codes(&"base".into(), &"quote".into());
        // Construct main object
        let trade_place_account = TradePlaceAccount::new(
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
        local_snapshots.insert(
            trade_place_account.trade_place(),
            primary_order_book_snapshot,
        );

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
            event::EventType::Update,
            order_book_data::OrderBookData::new(asks, bids),
        );

        // Perform update
        let trade_place = snapshot_controller
            .update(order_book_event)
            .expect("in test")
            .trade_place();

        let updated_asks = &snapshot_controller
            .get_snapshot(trade_place.clone())
            .expect("in test")
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(trade_place.clone())
            .expect("in test")
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
