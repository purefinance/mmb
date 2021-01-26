use super::exchanges::common::*;
use crate::core::order_book_data::OrderBookData;
use crate::core::DateTime;
use derive_getters::Dissolve;

#[derive(Copy, Clone)]
pub enum EventType {
    Snapshot,
    Update,
}

#[derive(Dissolve, Clone)]
pub struct OrderBookEvent {
    id: u128,
    creation_time: DateTime,
    exchange_id: ExchangeId,
    exchange_name: ExchangeName,
    currency_code_pair: CurrencyCodePair,

    event_id: String,

    event_type: EventType,
    data: OrderBookData,
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

    #[cfg(test)]
    pub fn new_for_update_tests(
        exchange_name: ExchangeName,
        currency_code_pair: CurrencyCodePair,
        event_type: EventType,
        data: OrderBookData,
    ) -> Self {
        use chrono::Utc;
        OrderBookEvent {
            id: 0,
            creation_time: Utc::now(),
            exchange_id: ExchangeId::from(""),
            exchange_name,
            currency_code_pair,
            event_id: "".to_string(),

            event_type,
            data,
        }
    }

    pub fn apply_data_update(&mut self, updates: Vec<OrderBookData>) {
        self.data.update(updates);
    }
}
