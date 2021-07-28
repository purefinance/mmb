use super::orders::event::OrderEventType;
use anyhow::{Context, Result};
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::{
    exchanges::{
        common::{Amount, Price, TradePlaceAccount},
        events::ExchangeEvent,
    },
    infrastructure::spawn_future,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TradePlaceAccountStatistic {
    opened_orders_count: usize,
    canceled_orders_count: usize,
    partially_filled_orders_count: usize,
    fully_filled_orders_count: usize,
    summary_filled_amount: Amount,
    summary_commission: Price,
}

impl TradePlaceAccountStatistic {
    fn order_created(&mut self) {
        self.opened_orders_count += 1;
    }

    fn order_canceled(&mut self) {
        self.canceled_orders_count += 1;
    }

    fn order_partially_filled(&mut self) {
        self.partially_filled_orders_count += 1;
    }

    fn order_completely_filled(&mut self) {
        self.partially_filled_orders_count = self.partially_filled_orders_count.saturating_sub(1);
        self.fully_filled_orders_count += 1;
    }

    fn add_summary_filled_amount(&mut self, filled_amount: Amount) {
        self.summary_filled_amount += filled_amount;
    }

    fn add_summary_commission(&mut self, commission: Price) {
        self.summary_commission += commission;
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DispositionExecutorStatistic {
    skipped_events_amount: usize,
}

impl DispositionExecutorStatistic {
    fn new(skipped_events_amount: usize) -> Self {
        Self {
            skipped_events_amount,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct StatisticService {
    trade_place_stats: RwLock<HashMap<TradePlaceAccount, TradePlaceAccountStatistic>>,
    disposition_executor_data: Mutex<DispositionExecutorStatistic>,
}

impl StatisticService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            trade_place_stats: Default::default(),
            disposition_executor_data: Default::default(),
        })
    }

    pub(crate) fn order_created(&self, trade_place_account: &TradePlaceAccount) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_created();
    }

    pub(crate) fn order_canceled(&self, trade_place_account: &TradePlaceAccount) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_canceled();
    }

    pub(crate) fn order_partially_filled(&self, trade_place_account: &TradePlaceAccount) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_partially_filled();
    }

    pub(crate) fn order_completely_filled(&self, trade_place_account: &TradePlaceAccount) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_completely_filled();
    }

    pub(crate) fn add_summary_amount(
        &self,
        trade_place_account: &TradePlaceAccount,
        filled_amount: Amount,
    ) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .add_summary_filled_amount(filled_amount);
    }

    pub(crate) fn add_summary_commission(
        &self,
        trade_place_account: &TradePlaceAccount,
        commission: Price,
    ) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .add_summary_commission(commission);
    }

    pub(crate) fn event_missed(&self) {
        (*self.disposition_executor_data.lock()).skipped_events_amount += 1;
    }
}

pub struct StatisticEventHandler {
    pub(crate) stats: StatisticService,
}

impl StatisticEventHandler {
    pub fn new(events_receiver: broadcast::Receiver<ExchangeEvent>) -> Arc<Self> {
        let statistic_event_handler = Arc::new(Self {
            stats: StatisticService::default(),
        });

        let cloned_self = statistic_event_handler.clone();
        let action = cloned_self.clone().start(events_receiver);
        spawn_future("Start statistic service", true, action.boxed());

        statistic_event_handler
    }

    pub async fn start(
        self: Arc<Self>,
        mut events_receiver: broadcast::Receiver<ExchangeEvent>,
    ) -> Result<()> {
        loop {
            let event = events_receiver
                .recv()
                .await
                .context("Error during receiving event in DispositionExecutor::start()")?;
            // There is no need to stop StatisticEventHandler via CancellationToken now
            // Better to collect all statistics, even events occur during graceful_shutdown
            // But then statistic future will work until tokio runtime is up

            self.handle_event(event)?;
        }
    }

    fn handle_event(&self, event: ExchangeEvent) -> Result<()> {
        match event {
            ExchangeEvent::OrderEvent(order_event) => {
                let trade_place_account = TradePlaceAccount::new(
                    order_event.order.exchange_account_id(),
                    order_event.order.currency_pair(),
                );
                match order_event.event_type {
                    OrderEventType::CreateOrderSucceeded => {
                        self.stats.order_created(&trade_place_account);
                    }
                    OrderEventType::CancelOrderSucceeded => {
                        self.stats.order_canceled(&trade_place_account);
                    }
                    OrderEventType::CreateOrderFailed => {}
                    OrderEventType::OrderFilled { cloned_order } => {}
                    OrderEventType::OrderCompleted { cloned_order } => {}
                    OrderEventType::CancelOrderFailed => {}
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub(crate) fn event_missed(self: Arc<Self>) {
        // FIXME implement
        todo!()
    }
}
