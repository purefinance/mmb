use crate::core::orders::{
    fill::OrderFill,
    }
    order::{OrderEventType, OrderSnapshot},
use crate::core::exchanges::common::{
    Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price,
};
use crate::core::exchanges::general::exchange::{Exchange, OrderBookTop, PriceLevel};
use crate::core::order_book::event::OrderBookEvent;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::orders::event::OrderEvent;
use crate::core::orders::order::{OrderSide, OrderType};
use crate::core::orders::pool::OrderRef;
use crate::core::DateTime;
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub const CHANNEL_MAX_EVENTS_COUNT: usize = 200_000;

#[derive(Debug, Clone)]
pub struct ExchangeBalance {
    pub currency_code: CurrencyCode,
    pub balance: Decimal,
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
        }}
        
#[derive(Debug, Clone)]
pub struct ExchangeBalancesAndPositions {
    pub balances: Vec<ExchangeBalance>,
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
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub liq_price: Price,
    pub entry_price: Price,
    pub side: OrderSide,
    _private: (), // field base constructor shouldn't be accessible from other modules
}

impl LiquidationPriceEvent {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        liq_price: Price,
        entry_price: Price,
        side: OrderSide,
    ) -> Self {
        LiquidationPriceEvent {
            version: LIQUIDATION_PRICE_CURRENT_VERSION,
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

    pub async fn start(
        mut events_receiver: broadcast::Receiver<ExchangeEvent>,
        exchanges_map: HashMap<ExchangeAccountId, Arc<Exchange>>,
    ) -> Result<()> {
        let mut local_snapshots_service = LocalSnapshotsService::default();

        loop {
            let event = events_receiver
                .recv()
                .await
                .context("Error during receiving event in ExchangeEvents event loop")?;

            match event {
                ExchangeEvent::OrderBookEvent(order_book_event) => {
                    update_order_book_top_for_exchange(
                        order_book_event,
                        &mut local_snapshots_service,
                        &exchanges_map,
                    )
                }
                ExchangeEvent::OrderEvent(order_event) => {
                    if let OrderType::Liquidation = order_event.order.order_type() {
                        // TODO react on order liquidation
                    }
                }
                ExchangeEvent::BalanceUpdate(_) => {
                    // TODO add update exchange balance
                }
                ExchangeEvent::LiquidationPrice(_) => {}
                ExchangeEvent::Trades(_) => {}
            }
        }
    }
}

fn update_order_book_top_for_exchange(
    order_book_event: OrderBookEvent,
    local_snapshots_service: &mut LocalSnapshotsService,
    exchanges_map: &HashMap<ExchangeAccountId, Arc<Exchange>>,
) {
    let trade_place_account = local_snapshots_service.update(order_book_event);
    if let Some(trade_place_account) = &trade_place_account {
        let snapshot = local_snapshots_service
            .get_snapshot(trade_place_account.clone())
            .expect("snapshot should exists because we just added one");

        let order_book_top = OrderBookTop {
            top_ask: snapshot
                .get_top_ask()
                .map(|(price, amount)| PriceLevel { price, amount }),
            top_bid: snapshot
                .get_top_bid()
                .map(|(price, amount)| PriceLevel { price, amount }),
        };

        exchanges_map
            .get(&trade_place_account.exchange_account_id)
            .map(|exchange| {
                exchange
                    .order_book_top
                    .insert(trade_place_account.currency_pair.clone(), order_book_top)
            });
    }
}
