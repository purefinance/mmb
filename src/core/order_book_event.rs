use super::exchanges::common::*;
use crate::core::order_book_data::OrderBookData;
use crate::core::DateTime;
use derive_getters::Getters;

#[derive(Clone)]
pub enum EventType {
    Snapshot,
    Update,
}

#[derive(Getters, Clone)]
pub struct OrderBookEvent {
    pub id: u128,
    pub creation_time: DateTime,
    pub exchange_id: ExchangeId,
    pub exchange_name: ExchangeName,
    pub currency_code_pair: CurrencyCodePair,

    pub event_id: String,

    pub event_type: EventType,
    pub data: OrderBookData,
}

impl OrderBookEvent {
    pub fn new(
        creation_time: DateTime,
        exchange_id: ExchangeId,
        exchange_name: ExchangeName,
        currency_code_pair: CurrencyCodePair,
        event_id: String,
        event_type: EventType,
        data: OrderBookData,
    ) -> OrderBookEvent {
        OrderBookEvent {
            id: 0,
            creation_time,
            exchange_id,
            exchange_name,
            currency_code_pair,
            event_id,
            event_type,
            data,
        }
    }

    pub fn apply_data_update(&mut self, updates: Vec<OrderBookData>) {
        self.data.update(updates);
    }
}
