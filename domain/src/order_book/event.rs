use mmb_utils::DateTime;

use crate::market::CurrencyPair;
use crate::market::*;
use crate::order_book::local_order_book_snapshot::LocalOrderBookSnapshot;
use crate::order_book::order_book_data::OrderBookData;
use std::sync::Arc;

/// Possible variants of OrderBookEvent
#[derive(Debug, Copy, Clone)]
pub enum EventType {
    /// Means full snapshot should be add to local snapshots
    Snapshot,
    /// Means that data should be applied to suitable existing snapshot
    Update,
}

/// Event to update local snapshot
#[derive(Debug, Clone)]
pub struct OrderBookEvent {
    _id: u128,
    pub creation_time: DateTime,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,

    _event_id: String,

    pub event_type: EventType,
    pub data: Arc<OrderBookData>,
}

impl OrderBookEvent {
    pub fn new(
        creation_time: DateTime,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        _event_id: String,
        event_type: EventType,
        data: Arc<OrderBookData>,
    ) -> OrderBookEvent {
        OrderBookEvent {
            _id: 0,
            creation_time,
            exchange_account_id,
            currency_pair,
            _event_id,
            event_type,
            data,
        }
    }

    pub fn market_account_id(&self) -> MarketAccountId {
        MarketAccountId::new(self.exchange_account_id, self.currency_pair)
    }

    pub fn to_orderbook_snapshot(&self) -> LocalOrderBookSnapshot {
        self.data.to_orderbook_snapshot(self.creation_time)
    }
}
