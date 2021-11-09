use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use log::warn;
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot};

use crate::core::balance_manager::balance_manager::BalanceManager;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::events::ExchangeEvent;
use crate::core::exchanges::general::exchange::{Exchange, OrderBookTop, PriceLevel};
use crate::core::lifecycle::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::Service;
use crate::core::order_book::event::OrderBookEvent;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::orders::order::OrderType;

pub(crate) struct InternalEventsLoop {
    work_finished_receiver: Mutex<Option<oneshot::Receiver<Result<()>>>>,
    pub balance_manager: Arc<Mutex<BalanceManager>>,
}

impl InternalEventsLoop {
    pub(crate) fn new(balance_manager: Arc<Mutex<BalanceManager>>) -> Arc<Self> {
        Arc::new(InternalEventsLoop {
            work_finished_receiver: Default::default(),
            balance_manager,
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
            .get_snapshot(trade_place_account.trade_place())
            .expect("snapshot should exists because we just added one");

        let order_book_top = OrderBookTop {
            ask: snapshot
                .get_top_ask()
                .map(|(price, amount)| PriceLevel { price, amount }),
            bid: snapshot
                .get_top_bid()
                .map(|(price, amount)| PriceLevel { price, amount }),
        };

        exchanges_map
            .get(&trade_place_account.exchange_account_id)
            .map(|exchange| {
                exchange
                    .order_book_top
                    .insert(trade_place_account.currency_pair, order_book_top)
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
            warn!("'work_finished_receiver' wasn't created when started graceful shutdown in InternalEventsLoop");
        }

        work_finished_receiver
    }
}
