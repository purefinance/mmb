use rust_decimal::Decimal;
use tokio::sync::broadcast;

use crate::core::exchanges::common::{
    Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price,
};
use crate::core::misc::derivative_position_info::DerivativePositionInfo;
use crate::core::order_book::event::OrderBookEvent;
use crate::core::orders::event::OrderEvent;
use crate::core::orders::order::OrderSide;
use crate::core::DateTime;

pub const CHANNEL_MAX_EVENTS_COUNT: usize = 200_000;

#[derive(Debug, Clone)]
pub struct ExchangeBalance {
    pub currency_code: CurrencyCode,
    pub balance: Decimal,
}

#[derive(Debug, Clone)]
pub struct ExchangeBalancesAndPositions {
    pub balances: Vec<ExchangeBalance>,
    pub positions: Option<Vec<DerivativePositionInfo>>,
}

#[derive(Debug, Clone)]
pub struct BalanceUpdateEvent {
    pub exchange_account_id: ExchangeAccountId,
    pub balances_and_positions: ExchangeBalancesAndPositions,
}

pub const LIQUIDATION_PRICE_CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct LiquidationPriceEvent {
    pub version: u32,
    pub event_creation_time: DateTime,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub liq_price: Price,
    pub entry_price: Price,
    pub side: OrderSide,
    _private: (), // field base constructor shouldn't be accessible from other modules
}

impl LiquidationPriceEvent {
    pub fn new(
        event_creation_time: DateTime,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        liq_price: Price,
        entry_price: Price,
        side: OrderSide,
    ) -> Self {
        LiquidationPriceEvent {
            version: LIQUIDATION_PRICE_CURRENT_VERSION,
            event_creation_time,
            exchange_account_id,
            currency_pair,
            liq_price,
            entry_price,
            side,
            _private: (),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TickDirection {
    None,
    ZeroMinusTick,
    MinusTick,
    ZeroPlusTick,
    PlusTick,
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub trade_id: String,
    pub price: Price,
    pub quantity: Amount,
    pub side: OrderSide,
    pub transaction_time: DateTime,
    pub tick_direction: TickDirection,
}

#[derive(Debug, Clone)]
pub struct TradesEvent {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub trades: Vec<Trade>,
}

#[derive(Debug, Clone)]
pub enum ExchangeEvent {
    OrderBookEvent(OrderBookEvent),
    OrderEvent(OrderEvent),
    BalanceUpdate(BalanceUpdateEvent),
    LiquidationPrice(LiquidationPriceEvent),
    Trades(TradesEvent),
}

pub(crate) struct ExchangeEvents {
    events_sender: broadcast::Sender<ExchangeEvent>,
}

impl ExchangeEvents {
    pub fn new(events_sender: broadcast::Sender<ExchangeEvent>) -> Self {
        ExchangeEvents { events_sender }
    }

    pub fn get_events_channel(&self) -> broadcast::Receiver<ExchangeEvent> {
        self.events_sender.subscribe()
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum AllowedEventSourceType {
    All,
    FallbackOnly,
    NonFallback,
}

impl Default for AllowedEventSourceType {
    fn default() -> Self {
        AllowedEventSourceType::All
    }
}
