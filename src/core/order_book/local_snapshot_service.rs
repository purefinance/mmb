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
    pub fn update(&mut self, event: event::OrderBookEvent) -> Option<TradePlaceAccount> {
        let trade_place_account = event.trade_place_account();
        let trade_place = trade_place_account.trade_place();

        match event.event_type {
            event::EventType::Snapshot => {
                self.local_snapshots
                    .insert(trade_place, event.data.to_local_order_book_snapshot());
                Some(trade_place_account)
            }
            event::EventType::Update => {
                self.local_snapshots
                    .get_mut(&trade_place)
                    .map(move |snapshot| {
                        snapshot.apply_update(&event.data, event.creation_time);
                        trade_place_account
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
    use crate::order_book_data;
    use chrono::Utc;
    use rust_decimal_macros::*;
    use std::sync::Arc;

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
            Arc::new(order_book_data),
        )
    }

    #[test]
    fn update_by_full_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let order_book_data = order_book_data![
            dec!(1.0) => dec!(2.1),
            dec!(3.0) => dec!(4.2),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(3.4) => dec!(1.2),
        ];

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes("base".into(), "quote".into()),
            event::EventType::Snapshot,
            order_book_data,
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

        let order_book_data = order_book_data![
            dec!(1.0) => dec!(2.1),
            dec!(3.0) => dec!(4.2),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(3.4) => dec!(1.2),
        ];

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes("base".into(), "quote".into()),
            event::EventType::Update,
            order_book_data,
        );

        // Perform update
        let update_result = snapshot_service.update(order_book_event);

        // There was nothing to update
        assert!(update_result.is_none());
    }

    #[test]
    fn successful_update() {
        let test_exchange_id = "exchange_id";
        let test_currency_pair = CurrencyPair::from_codes("base".into(), "quote".into());
        // Construct main object
        let trade_place_account = TradePlaceAccount::new(
            ExchangeAccountId::new(test_exchange_id.into(), 0),
            test_currency_pair,
        );

        let primary_order_book_snapshot = order_book_data![
            dec!(1.0) => dec!(0.1),
            dec!(3.0) => dec!(4.2),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(3.4) => dec!(1.2),
        ]
        .to_local_order_book_snapshot();

        let mut local_snapshots = HashMap::new();
        local_snapshots.insert(
            trade_place_account.trade_place(),
            primary_order_book_snapshot,
        );

        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        let order_book_data = order_book_data![
            dec!(1.0) => dec!(2.1),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(3.4) => dec!(0),
        ];

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            test_exchange_id.into(),
            test_currency_pair,
            event::EventType::Update,
            order_book_data,
        );

        // Perform update
        let trade_place = snapshot_controller
            .update(order_book_event)
            .expect("in test")
            .trade_place();

        let updated_asks = &snapshot_controller
            .get_snapshot(trade_place)
            .expect("in test")
            .asks;

        let updated_bids = &snapshot_controller
            .get_snapshot(trade_place)
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
