use crate::core::*;

use super::exchanges::common::*;
use crate::core::local_order_book_snapshot::LocalOrderBookSnapshot;
use std::collections::HashMap;

pub struct LocalSnapshotsService {
    local_snapshots: HashMap<ExchangeNameSymbol, LocalOrderBookSnapshot>,
}

impl LocalSnapshotsService {
    fn new(local_snapshots: HashMap<ExchangeNameSymbol, LocalOrderBookSnapshot>) -> Self {
        Self { local_snapshots }
    }

    fn get_snapshot(&self, snaphot_id: ExchangeNameSymbol) -> Option<&LocalOrderBookSnapshot> {
        self.local_snapshots.get(&snaphot_id)
    }

    fn update(
        &mut self,
        order_book_event: order_book_event::OrderBookEvent,
    ) -> Option<ExchangeIdSymbol> {
        let exchange_symbol = ExchangeIdSymbol::new(
            order_book_event.exchange_id,
            order_book_event.exchange_name,
            order_book_event.currency_code_pair,
        );

        match order_book_event.event_type {
            order_book_event::EventType::Snapshot => {
                let local_order_book_snapshot =
                    order_book_event.data.to_local_order_book_snapshot();
                let exchanger_currency_state = exchange_symbol.get_exchanger_currency_state();

                self.local_snapshots
                    .insert(exchanger_currency_state.clone(), local_order_book_snapshot);

                return Some(exchange_symbol);
            }
            order_book_event::EventType::Update => {
                // Exctract variablse here to avoid partial moving
                let event_data = order_book_event.data;
                let event_datetime = order_book_event.creation_time;
                self.local_snapshots
                    .get_mut(&exchange_symbol.get_exchanger_currency_state())
                    .map(|snapshot| {
                        snapshot.apply_update(event_data, event_datetime);
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
    use order_book_data::OrderDataMap;
    use rust_decimal_macros::*;
    use smallstr::SmallString;
    use std::collections::BTreeMap;

    fn get_raw_order_book_event() -> order_book_event::OrderBookEvent {
        order_book_event::OrderBookEvent {
            id: 0,
            creation_time: Utc::now(),
            exchange_id: ExchangeId::from(""),
            exchange_name: ExchangeName::from(""),
            currency_code_pair: CurrencyCodePair::new(SmallString::new()),

            event_id: "".to_string(),

            event_type: order_book_event::EventType::Snapshot,
            data: order_book_data::OrderBookData {
                asks: BTreeMap::new(),
                bids: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn update_by_full_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        // TODO add values to map above. Just to control that they'll be changed
        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        // Construct update
        let mut order_book_event = get_raw_order_book_event();

        let mut asks = OrderDataMap::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = OrderDataMap::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));
        order_book_event.data = order_book_data::OrderBookData::new(asks, bids);
        order_book_event.event_type = order_book_event::EventType::Snapshot;

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
        // TODO add values to map above. Just to control that they'll be changed
        let mut snapshot_controller = LocalSnapshotsService::new(local_snapshots);

        // Construct update
        let mut order_book_event = get_raw_order_book_event();

        let mut asks = OrderDataMap::new();
        asks.insert(dec!(1.0), dec!(2.1));
        asks.insert(dec!(3.0), dec!(4.2));
        let mut bids = OrderDataMap::new();
        bids.insert(dec!(2.9), dec!(7.8));
        bids.insert(dec!(3.4), dec!(1.2));
        order_book_event.data = order_book_data::OrderBookData::new(asks, bids);
        order_book_event.event_type = order_book_event::EventType::Update;

        // Perform update
        let update_result = snapshot_controller.update(order_book_event);

        // There was nothing to update
        assert!(update_result.is_none());
    }

    #[test]
    fn successful_update() {
        let test_phrase = "test";
        // Construct main object
        let exchange_name_symbol = ExchangeNameSymbol::new(
            ExchangeName::new(test_phrase.into()),
            CurrencyCodePair::new(test_phrase.into()),
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

        // Construct update
        let mut order_book_event = get_raw_order_book_event();

        let mut asks = OrderDataMap::new();
        asks.insert(dec!(1.0), dec!(2.1));
        let mut bids = OrderDataMap::new();
        bids.insert(dec!(2.9), dec!(7.8));
        order_book_event.data = order_book_data::OrderBookData::new(asks, bids);
        order_book_event.event_type = order_book_event::EventType::Update;

        // Mathching will be based on this data
        order_book_event.exchange_name = ExchangeName::new(test_phrase.into());
        order_book_event.currency_code_pair = CurrencyCodePair::new(test_phrase.into());

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
            // Not updated
            Some(&dec!(1.2))
        );
    }
}
