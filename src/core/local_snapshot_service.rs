use order_book_event::{OrderBookEvent, OrderBookEventType};

use super::exchanges::common::*;
use std::collections::HashMap;
// TODO у меня какая-то проблема с подключением модулей и правильной иерархией вообще. Евгений, хелп
use crate::core::local_order_book_snapshot::LocalOrderBookSnapshot;
use crate::core::*;

// TODO Как бы назвать правильно эту пару (ID обменника + валютная пара) ExchangerCurrencyPairState?
#[derive(PartialEq, Eq, Hash)]
struct ExchangeNameSymbol {
    exchange_name: ExchangeName,
    currency_code_pair: CurrencyCodePair,
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
    ) -> Option<(LocalOrderBookSnapshot, ExchangeIdSymbol)>;
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
    ) -> Option<(LocalOrderBookSnapshot, ExchangeIdSymbol)> {
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

                self.local_snapshots.insert(
                    exchange_symbol.get_exchanger_currency_state(),
                    local_order_book_snapshot.clone(),
                );
                return Some((local_order_book_snapshot, exchange_symbol));
            }
            OrderBookEventType::Update => {
                match self
                    .local_snapshots
                    .get_mut(&exchange_symbol.get_exchanger_currency_state())
                {
                    Some(snapshot) => {
                        snapshot.apply_update(order_book_event.data, order_book_event.datetime);
                        let result_snaphot = snapshot.clone();
                        return Some((result_snaphot, exchange_symbol));
                    }
                    None => return None,
                }
            }
        }
    }
}
