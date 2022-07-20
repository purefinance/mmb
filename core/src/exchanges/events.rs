use core::panic;
use std::fmt::{Display, Formatter};

use mmb_utils::DateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::exchanges::common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price};
use crate::misc::derivative_position::DerivativePosition;
use crate::order_book::event::OrderBookEvent;
use crate::orders::event::OrderEvent;
use crate::orders::order::OrderSide;

pub const CHANNEL_MAX_EVENTS_COUNT: usize = 200_000;

#[derive(Debug, Clone)]
pub struct ExchangeBalance {
    pub currency_code: CurrencyCode,
    pub balance: Decimal,
}

#[derive(Debug, Clone)]
pub struct ExchangeBalancesAndPositions {
    pub balances: Vec<ExchangeBalance>,
    pub positions: Option<Vec<DerivativePosition>>,
}

#[derive(Debug, Clone)]
pub struct BalanceUpdateEvent {
    pub exchange_account_id: ExchangeAccountId,
    pub balances_and_positions: ExchangeBalancesAndPositions,
}

pub const LIQUIDATION_PRICE_CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LiquidationPriceEvent {
    pub version: u32,
    pub event_creation_time: DateTime,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub liq_price: Price,
    pub entry_price: Price,
    pub side: OrderSide,
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub enum TradeId {
    Number(u64),
    String(Box<str>),
}

impl TradeId {
    pub fn get_number(&self) -> u64 {
        match self {
            TradeId::Number(number) => *number,
            TradeId::String(_) => {
                panic!("Unable to get number from string trade id")
            }
        }
    }
}

impl From<Value> for TradeId {
    fn from(value: Value) -> Self {
        match value.as_u64() {
            Some(value) => TradeId::Number(value),
            None => TradeId::String(value.to_string().into_boxed_str()),
        }
    }
}

impl PartialEq for TradeId {
    fn eq(&self, other: &TradeId) -> bool {
        let panic_msg = "TradeId formats don't match";
        match self {
            TradeId::Number(this) => match other {
                TradeId::Number(other) => this == other,
                TradeId::String(_) => panic!("{}", panic_msg),
            },
            TradeId::String(this) => match other {
                TradeId::Number(_) => panic!("{}", panic_msg),
                TradeId::String(other) => this == other,
            },
        }
    }
}

impl Display for TradeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeId::Number(number) => {
                write!(f, "{}", number)
            }
            TradeId::String(string) => {
                write!(f, "{}", string)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub trade_id: TradeId,
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
    pub receipt_time: DateTime,
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

#[derive(Debug, Default, Clone, PartialEq, Copy)]
pub enum AllowedEventSourceType {
    #[default]
    All,
    FallbackOnly,
    NonFallback,
}
