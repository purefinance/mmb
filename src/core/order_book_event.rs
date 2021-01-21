use super::exchanges::common::*;
use crate::core::order_book_data::OrderBookData;

use chrono::Utc;
type DateTime = chrono::DateTime<Utc>;

// TODO reduce usin OrderBookEvent just cause namespace: order_book_event::type
pub enum OrderBookEventType {
    Snapshot,
    Update,
}

pub struct OrderBookEvent {
    // TODO is that suitable type?
    // Этот айди изи IMarketData и StorageItem. А он нужен вообще?
    pub id: u128,
    pub datetime: DateTime,
    pub exchange_id: ExchangeId,
    pub exchange_name: ExchangeName,
    pub currency_pair: CurrencyCodePair,

    pub event_id: String,

    event_type: OrderBookEventType,
    order_book_data: OrderBookData,
}

impl OrderBookEvent {
    pub fn new(
        datetime: DateTime,
        exchange_id: ExchangeId,
        exchange_name: ExchangeName,
        currency_pair: CurrencyCodePair,
        event_id: String,
        event_type: OrderBookEventType,
        order_book_data: OrderBookData,
    ) -> OrderBookEvent {
        OrderBookEvent {
            id: 0,
            datetime,
            exchange_id,
            exchange_name,
            currency_pair,
            event_id,
            event_type,
            order_book_data,
        }
    }

    pub fn apply_data_update(&mut self, updates: Vec<OrderBookData>) {
        self.order_book_data.update(updates);
    }
}
