use mmb_domain::market::{MarketAccountId, MarketId};
use mmb_domain::order_book::event;
use mmb_domain::order_book::local_order_book_snapshot::{LocalOrderBookSnapshot, ResultAskBidFix};
use mmb_utils::infrastructure::WithExpect;
use std::collections::HashMap;

/// Produce and actualize current logical state of order book snapshot according to logical time of handled order book events
pub struct LocalSnapshotsService {
    local_snapshots: HashMap<MarketId, LocalOrderBookSnapshot>,
}

impl LocalSnapshotsService {
    pub fn new(local_snapshots: HashMap<MarketId, LocalOrderBookSnapshot>) -> Self {
        Self { local_snapshots }
    }

    pub fn get_snapshot(&self, market_id: MarketId) -> Option<&LocalOrderBookSnapshot> {
        self.local_snapshots.get(&market_id)
    }

    pub fn get_snapshot_expected(&self, market_id: MarketId) -> &LocalOrderBookSnapshot {
        self.local_snapshots
            .get(&market_id)
            .with_expect(|| format!("Can't get snapshot for {:?}", market_id))
    }

    /// Create snapshot if it does not exist
    /// Update snapshot if suitable data arrive
    /// Returns `Some(MarketAccountId)` if snapshot update succeeded, otherwise `None`
    pub fn update(&mut self, event: &event::OrderBookEvent) -> Option<MarketAccountId> {
        let market_account_id = event.market_account_id();
        let market_id = market_account_id.market_id();

        match event.event_type {
            event::EventType::Snapshot => {
                let mut snapshot = event.to_orderbook_snapshot();
                if let ResultAskBidFix::Fixed { top_ask, top_bid } =
                    snapshot.fix_asks_bids_if_needed()
                {
                    log::warn!("On {market_account_id} orderbook top asks {top_ask} and bids {top_bid} was crossed (fixed now {})", snapshot.get_top_prices())
                }

                self.local_snapshots.insert(market_id, snapshot);

                Some(market_account_id)
            }
            event::EventType::Update => match self.local_snapshots.get_mut(&market_id) {
                None => None,
                Some(snapshot) => {
                    snapshot.apply_update(&event.data, event.creation_time);

                    if let ResultAskBidFix::Fixed { top_ask, top_bid } =
                        snapshot.fix_asks_bids_if_needed()
                    {
                        log::warn!("On {market_account_id} orderbook top asks {top_ask} and bids {top_bid} was crossed (fixed now {})", snapshot.get_top_prices())
                    }

                    Some(market_account_id)
                }
            },
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
    use mmb_domain::market::{CurrencyPair, ExchangeAccountId, ExchangeId};
    use mmb_domain::order_book::order_book_data;
    use mmb_domain::order_book_data;
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
        let mut snapshot_service = LocalSnapshotsService::new(local_snapshots);

        let order_book_data = order_book_data![
            dec!(3.4) => dec!(1.2),
            dec!(3.0) => dec!(4.2),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(1.0) => dec!(2.1),
        ];

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes("base".into(), "quote".into()),
            event::EventType::Snapshot,
            order_book_data.clone(),
        );

        // Perform update
        let market_account_id = snapshot_service.update(&order_book_event).expect("in test");

        // Check all snapshot

        let snapshot = &snapshot_service
            .get_snapshot(market_account_id.market_id())
            .expect("in test");

        assert_eq!(snapshot.asks, order_book_data.asks);
        assert_eq!(snapshot.bids, order_book_data.bids);
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
        let update_result = snapshot_service.update(&order_book_event);

        // There was nothing to update
        assert!(update_result.is_none());
    }

    #[test]
    fn successful_update() {
        let test_exchange_id = "exchange_id";
        let test_currency_pair = CurrencyPair::from_codes("base".into(), "quote".into());
        // Construct main object
        let market_account_id = MarketAccountId::new(
            ExchangeAccountId::new(test_exchange_id, 0),
            test_currency_pair,
        );

        let primary_order_book_snapshot = order_book_data![
            dec!(3.4) => dec!(1.2),
            dec!(3.0) => dec!(4.2),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(1.0) => dec!(0.1),
        ]
        .to_orderbook_snapshot(Utc::now());

        let mut local_snapshots = HashMap::new();
        local_snapshots.insert(market_account_id.market_id(), primary_order_book_snapshot);

        let mut snapshot_service = LocalSnapshotsService::new(local_snapshots);

        let order_book_data = order_book_data![
            dec!(3.4) => dec!(0),
            ;
            dec!(2.9) => dec!(7.8),
            dec!(1.0) => dec!(2.1),
        ];

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            test_exchange_id.into(),
            test_currency_pair,
            event::EventType::Update,
            order_book_data,
        );

        // Perform update
        let market_id = snapshot_service
            .update(&order_book_event)
            .expect("in test")
            .market_id();

        // Check all snapshot
        let expected = order_book_data![
            // price level 3.4 - deleted
            dec!(3.0) => dec!(4.2), // not changed
            ;
            dec!(2.9) => dec!(7.8), // updated
            dec!(1.0) => dec!(2.1), // updated
        ];

        let snapshot = &snapshot_service.get_snapshot(market_id).expect("in test");

        assert_eq!(snapshot.asks, expected.asks);
        assert_eq!(snapshot.bids, expected.bids);
    }

    #[test]
    fn update_and_fix_by_full_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        let mut snapshot_service = LocalSnapshotsService::new(local_snapshots);

        let order_book_data = order_book_data![
            dec!(3.4) => dec!(1.2),
            dec!(2.9) => dec!(7.8),
            ;
            dec!(3.0) => dec!(4.2),
            dec!(1.0) => dec!(2.1),
        ];

        // Construct update
        let order_book_event = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes("base".into(), "quote".into()),
            event::EventType::Snapshot,
            order_book_data.clone(),
        );

        // Perform update
        let market_account_id = snapshot_service.update(&order_book_event).expect("in test");

        let snapshot = &snapshot_service
            .get_snapshot(market_account_id.market_id())
            .expect("in test");

        // Check all snapshot
        let expected = order_book_data![
            dec!(3.4) => dec!(1.2),
            ;
            dec!(1.0) => dec!(2.1),
        ];
        assert_eq!(snapshot.asks, expected.asks);
        assert_eq!(snapshot.bids, expected.bids);
    }

    #[test]
    fn update_and_fix_by_orderbook_update() {
        // Construct main object
        let local_snapshots = HashMap::new();
        let mut snapshot_service = LocalSnapshotsService::new(local_snapshots);

        let order_book_data_snapshot = order_book_data![
            dec!(3.4) => dec!(1.2),
            dec!(2.9) => dec!(7.8),
            ;
            dec!(1.5) => dec!(4.2),
            dec!(1.0) => dec!(2.1),
        ];

        let order_book_event_snapshot = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes("base".into(), "quote".into()),
            event::EventType::Snapshot,
            order_book_data_snapshot.clone(),
        );

        snapshot_service
            .update(&order_book_event_snapshot)
            .expect("in test");

        let order_book_data_update = order_book_data![
            ;
            dec!(3.1) => dec!(5.7),
        ];

        let order_book_event_update = create_order_book_event_for_tests(
            "does_not_matter".into(),
            CurrencyPair::from_codes("base".into(), "quote".into()),
            event::EventType::Update,
            order_book_data_update.clone(),
        );

        // Perform update
        let market_account_id = snapshot_service
            .update(&order_book_event_update)
            .expect("in test");

        // Check all snapshot
        let expected = order_book_data![
            dec!(3.4) => dec!(1.2),
            ;
            dec!(1.5) => dec!(4.2),
            dec!(1.0) => dec!(2.1),
        ];

        let snapshot = &snapshot_service
            .get_snapshot(market_account_id.market_id())
            .expect("in test");

        assert_eq!(snapshot.asks, expected.asks);
        assert_eq!(snapshot.bids, expected.bids);
    }
}
