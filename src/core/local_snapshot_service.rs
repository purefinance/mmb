use super::exchanges::common::*;
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

// TODO rename to LocalSnapshotsController
trait LocalSnapshotService {
    //fn get_snapshot(
    //    local_order_book_snapshot: LocalOrderBookSnapshot,
    //) -> Option<LocalOrderBookSnapshot> {
    //}

    //fn update(order_book_event: OrderBookEvent) -> (LocalOrderBookSnapshot, ExchangeIdSymbol) {}
}
