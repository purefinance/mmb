use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::nothing_to_do;
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot};

use crate::exchanges::common::ExchangeAccountId;
use crate::exchanges::events::ExchangeEvent;
use crate::exchanges::general::exchange::{Exchange, OrderBookTop, PriceLevel};
use crate::lifecycle::trading_engine::Service;
use crate::order_book::event::OrderBookEvent;
use crate::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::orders::event::OrderEventType;
use crate::orders::order::OrderType;

pub(crate) struct InternalEventsLoop {
    work_finished_receiver: Mutex<Option<oneshot::Receiver<Result<()>>>>,
}

impl InternalEventsLoop {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(InternalEventsLoop {
            work_finished_receiver: Default::default(),
        })
    }

    pub async fn start(
        self: Arc<Self>,
        mut events_receiver: broadcast::Receiver<ExchangeEvent>,
        exchanges_map: HashMap<ExchangeAccountId, Arc<Exchange>>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        let mut local_snapshots_service = LocalSnapshotsService::default();
        let (work_finished_sender, receiver) = oneshot::channel();
        *self.work_finished_receiver.lock() = Some(receiver);

        loop {
            let event = tokio::select! {
                event_res = events_receiver.recv() => event_res.context("Error during receiving event in InternalEventsLoop::start()")?,
                _ = cancellation_token.when_cancelled() => {
                    let _ = work_finished_sender.send(Ok(()));
                    return Ok(());
                }
            };

            match event {
                ExchangeEvent::OrderBookEvent(order_book_event) => {
                    update_order_book_top_for_exchange(
                        order_book_event,
                        &mut local_snapshots_service,
                        &exchanges_map,
                    )
                }
                ExchangeEvent::OrderEvent(order_event) => {
                    let target_eai = order_event.order.exchange_account_id();
                    let exchange = exchanges_map
                        .get(&target_eai)
                        .with_expect(|| format!("Failed to get Exchange for {}", target_eai));

                    match order_event.event_type {
                        OrderEventType::CreateOrderSucceeded => {
                            exchange.order_created_notify(&order_event.order);
                        }
                        OrderEventType::CreateOrderFailed => {
                            exchange.order_created_notify(&order_event.order);
                            exchange.order_finished_notify(&order_event.order);
                        }
                        OrderEventType::CancelOrderSucceeded
                        | OrderEventType::OrderCompleted { .. } => {
                            exchange.order_finished_notify(&order_event.order);
                        }
                        _ => nothing_to_do(),
                    }
                    if let OrderType::Liquidation = order_event.order.order_type() {
                        // TODO react on order liquidation
                    }
                }
                ExchangeEvent::BalanceUpdate(_) => {}
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
    let market_account_id = local_snapshots_service.update(order_book_event);
    if let Some(market_account_id) = &market_account_id {
        let snapshot = local_snapshots_service.get_snapshot_expected(market_account_id.market_id());

        let order_book_top = OrderBookTop {
            ask: snapshot
                .get_top_ask()
                .map(|(price, amount)| PriceLevel { price, amount }),
            bid: snapshot
                .get_top_bid()
                .map(|(price, amount)| PriceLevel { price, amount }),
        };

        exchanges_map
            .get(&market_account_id.exchange_account_id)
            .map(|exchange| {
                exchange
                    .order_book_top
                    .insert(market_account_id.currency_pair, order_book_top)
            });
    }
}

impl Service for InternalEventsLoop {
    fn name(&self) -> &str {
        "InternalEventsLoop"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>> {
        let work_finished_receiver = self.work_finished_receiver.lock().take();
        if work_finished_receiver.is_none() {
            log::warn!("'work_finished_receiver' wasn't created when started graceful shutdown in InternalEventsLoop");
        }

        work_finished_receiver
    }
}
