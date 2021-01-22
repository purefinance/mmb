use crate::core::*;
use order_book_event::{OrderBookEvent, OrderBookEventType};

use super::exchanges::common::*;
use std::collections::HashMap;
// TODO у меня какая-то проблема с подключением модулей и правильной иерархией вообще. Евгений, хелп
use crate::core::local_order_book_snapshot::LocalOrderBookSnapshot;

// TODO Как бы назвать правильно эту пару (ID обменника + валютная пара) ExchangerCurrencyPairState?
#[derive(PartialEq, Eq, Hash, Clone)]
struct ExchangeNameSymbol {
    exchange_name: ExchangeName,
    currency_code_pair: CurrencyCodePair,
}

impl ExchangeNameSymbol {
    fn new(exchange_name: ExchangeName, currency_code_pair: CurrencyCodePair) -> Self {
        Self {
            exchange_name,
            currency_code_pair,
        }
    }
}

struct ExchangeIdSymbol {
    exchange_id: ExchangeId,
    exchange_name: ExchangeName,
    currency_code_pair: CurrencyCodePair,
}

impl ExchangeIdSymbol {
    fn new(
        exchange_id: ExchangeId,
        exchange_name: ExchangeName,
        currency_code_pair: CurrencyCodePair,
    ) -> Self {
        Self {
            exchange_id,
            exchange_name,
            currency_code_pair,
        }
    }

    pub fn get_exchanger_currency_state(&self) -> ExchangeNameSymbol {
        ExchangeNameSymbol {
            currency_code_pair: self.currency_code_pair.clone(),
            exchange_name: self.exchange_name.clone(),
        }
    }
}

// TODO that was ILocalSnapshotService
trait ControlLocalSnapshots {
    //fn get_snapshot(
    //    local_order_book_snapshot: LocalOrderBookSnapshot,
    //) -> Option<LocalOrderBookSnapshot>;

    fn update(
        &mut self,
        order_book_event: OrderBookEvent,
    ) -> Option<(&LocalOrderBookSnapshot, ExchangeIdSymbol)>;
}

// TODO that was LocalSnapshotService
struct LocalSnapshotsController {
    local_snapshots: HashMap<ExchangeNameSymbol, LocalOrderBookSnapshot>,
}

impl LocalSnapshotsController {
    fn new(local_snapshots: HashMap<ExchangeNameSymbol, LocalOrderBookSnapshot>) -> Self {
        Self { local_snapshots }
    }
}

impl ControlLocalSnapshots for LocalSnapshotsController {
    fn update(
        &mut self,
        order_book_event: order_book_event::OrderBookEvent,
    ) -> Option<(&LocalOrderBookSnapshot, ExchangeIdSymbol)> {
        // TODO Bad name!
        let exchange_symbol = ExchangeIdSymbol::new(
            order_book_event.exchange_id,
            order_book_event.exchange_name,
            order_book_event.currency_code_pair,
        );

        match order_book_event.event_type {
            OrderBookEventType::Snapshot => {
                let local_order_book_snapshot =
                    order_book_event.data.to_local_order_book_snapshot();
                let exchanger_currency_state = exchange_symbol.get_exchanger_currency_state();

                self.local_snapshots
                    .insert(exchanger_currency_state.clone(), local_order_book_snapshot);

                return Some((
                    self.local_snapshots
                        .get(&exchanger_currency_state)
                        // unwrap here is safe because that value was just inserted
                        .unwrap(),
                    exchange_symbol,
                ));
            }
            OrderBookEventType::Update => {
                match self
                    .local_snapshots
                    .get_mut(&exchange_symbol.get_exchanger_currency_state())
                {
                    Some(snapshot) => {
                        snapshot.apply_update(order_book_event.data, order_book_event.datetime);
                        return Some((snapshot, exchange_symbol));
                    }
                    None => return None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use order_book_data::OrderDataMap;
    use rust_decimal::prelude::*;

    #[test]
    fn update_by_full_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        // TODO add values to map above. Just to control that they'll be changed
        let mut snapshot_controller = LocalSnapshotsController::new(local_snapshots);

        // Construct update
        let mut order_book_event = order_book_event::OrderBookEvent::new_raw();

        let mut asks = OrderDataMap::new();
        asks.insert(Decimal::new(1, 0), Decimal::new(2, 1));
        asks.insert(Decimal::new(3, 0), Decimal::new(4, 2));
        let mut bids = OrderDataMap::new();
        bids.insert(Decimal::new(2, 9), Decimal::new(7, 8));
        bids.insert(Decimal::new(3, 4), Decimal::new(1, 2));
        order_book_event.data = order_book_data::OrderBookData::new(asks, bids);
        order_book_event.event_type = order_book_event::OrderBookEventType::Snapshot;

        // Perform update
        let (local_order_book_snapshot, _exchange_id_symbol) =
            snapshot_controller.update(order_book_event).unwrap();

        // Check all snapshot returned values
        assert_eq!(
            local_order_book_snapshot.asks.get(&Decimal::new(1, 0)),
            Some(&Decimal::new(2, 1))
        );
        assert_eq!(
            local_order_book_snapshot.asks.get(&Decimal::new(3, 0)),
            Some(&Decimal::new(4, 2))
        );
        assert_eq!(
            local_order_book_snapshot.bids.get(&Decimal::new(2, 9)),
            Some(&Decimal::new(7, 8))
        );
        assert_eq!(
            local_order_book_snapshot.bids.get(&Decimal::new(3, 4)),
            Some(&Decimal::new(1, 2))
        );
    }

    #[test]
    fn update_if_no_such_snapshot() {
        // Construct main object
        let local_snapshots = HashMap::new();
        // TODO add values to map above. Just to control that they'll be changed
        let mut snapshot_controller = LocalSnapshotsController::new(local_snapshots);

        // Construct update
        let mut order_book_event = order_book_event::OrderBookEvent::new_raw();

        let mut asks = OrderDataMap::new();
        asks.insert(Decimal::new(1, 0), Decimal::new(2, 1));
        asks.insert(Decimal::new(3, 0), Decimal::new(4, 2));
        let mut bids = OrderDataMap::new();
        bids.insert(Decimal::new(2, 9), Decimal::new(7, 8));
        bids.insert(Decimal::new(3, 4), Decimal::new(1, 2));
        order_book_event.data = order_book_data::OrderBookData::new(asks, bids);
        order_book_event.event_type = order_book_event::OrderBookEventType::Update;

        // Perform update
        let update_result = snapshot_controller.update(order_book_event);

        // There was nothing to update
        assert!(update_result.is_none());
    }

    #[test]
    fn successful_update() {
        // Construct main object
        let exchange_name_symbol = ExchangeNameSymbol::new(
            ExchangeName::new("test".into()),
            CurrencyCodePair::new("test".into()),
        );

        let mut primary_asks = OrderDataMap::new();
        primary_asks.insert(Decimal::new(1, 0), Decimal::new(0, 1));
        primary_asks.insert(Decimal::new(3, 0), Decimal::new(4, 2));
        let mut primary_bids = OrderDataMap::new();
        primary_bids.insert(Decimal::new(2, 9), Decimal::new(7, 8));
        primary_bids.insert(Decimal::new(3, 4), Decimal::new(1, 2));

        let primary_order_book_snapshot =
            LocalOrderBookSnapshot::new(primary_asks, primary_bids, Utc::now());

        let mut local_snapshots = HashMap::new();
        local_snapshots.insert(exchange_name_symbol, primary_order_book_snapshot);

        let mut snapshot_controller = LocalSnapshotsController::new(local_snapshots);

        // Construct update
        let mut order_book_event = order_book_event::OrderBookEvent::new_raw();

        let mut asks = OrderDataMap::new();
        asks.insert(Decimal::new(1, 0), Decimal::new(2, 1));
        let mut bids = OrderDataMap::new();
        bids.insert(Decimal::new(2, 9), Decimal::new(7, 8));
        order_book_event.data = order_book_data::OrderBookData::new(asks, bids);
        order_book_event.event_type = order_book_event::OrderBookEventType::Update;

        // Mathching will be based on this data
        order_book_event.exchange_name = ExchangeName::new("test".into());
        order_book_event.currency_code_pair = CurrencyCodePair::new("test".into());

        // Perform update
        let (local_order_book_snapshot, _exchange_id_symbol) =
            snapshot_controller.update(order_book_event).unwrap();

        // Check all snapshot returned values
        assert_eq!(
            local_order_book_snapshot.asks.get(&Decimal::new(1, 0)),
            // Updated
            Some(&Decimal::new(2, 1))
        );
        assert_eq!(
            local_order_book_snapshot.asks.get(&Decimal::new(3, 0)),
            // Not updated
            Some(&Decimal::new(4, 2))
        );
        assert_eq!(
            local_order_book_snapshot.bids.get(&Decimal::new(2, 9)),
            // Updated
            Some(&Decimal::new(7, 8))
        );
        assert_eq!(
            local_order_book_snapshot.bids.get(&Decimal::new(3, 4)),
            // Not updated
            Some(&Decimal::new(1, 2))
        );
    }
}
