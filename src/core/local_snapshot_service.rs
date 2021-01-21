use super::exchanges::common::*;
use std::collections::HashMap;
// TODO у меня какая-то проблема с подключением модулей и правильной иерархией вообще. Евгений, хелп
use crate::core::local_order_book_snapshot::LocalOrderBookSnapshot;
use crate::core::order_book_event::OrderBookEvent;

struct ExchangeNameSymbol {
    currency_code_pair: CurrencyCodePair,
    exchange_name: ExchangeName,
}

struct ExchangeIdSymbol {
    exchange_id: ExchangeId,
    exchange_name: ExchangeName,
    currency_pair: CurrencyCodePair,
}

// TODO that was ILocalSnapshotService
trait ControlLocalSnapshots {
    //fn get_snapshot(
    //    local_order_book_snapshot: LocalOrderBookSnapshot,
    //) -> Option<LocalOrderBookSnapshot>;

    fn update(
        &mut self,
        order_book_event: OrderBookEvent,
    ) -> (LocalOrderBookSnapshot, ExchangeIdSymbol);
}

// TODO that was LocalSnapshotService
struct LocalSnapshotsController {
    local_snapshots: HashMap<ExchangeNameSymbol, LocalSnapshotsController>,
}

impl LocalSnapshotsController {
    fn new(local_snapshots: HashMap<ExchangeNameSymbol, LocalSnapshotsController>) -> Self {
        Self { local_snapshots }
    }
}

//impl ControlLocalSnapshots for LocalSnapshotsController {
//    fn update(
//        &mut self,
//        order_book_event: OrderBookEvent,
//    ) -> (LocalOrderBookSnapshot, ExchangeIdSymbol) {
//        match order_book_event.event_type {
//            Snapshot => {
//                let local_order_book_snapshot = order_book_event.data
//            }
//            Update => {}
//        }
//
//        (local_order_book_snapshot, ExchangeNameSymbol)
//    }
//}
