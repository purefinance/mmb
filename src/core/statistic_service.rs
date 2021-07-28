use super::orders::{event::OrderEventType, order::ClientOrderId};
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
    fn increment_created_orders(&mut self) {
        self.opened_orders_count += 1;
    }

    fn increment_canceled_orders(&mut self) {
        self.canceled_orders_count += 1;
    }

    fn increment_partially_filled_orders(&mut self) {
        self.partially_filled_orders_count += 1;
    }

    fn increment_completely_filled_orders(&mut self) {
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

    pub(crate) fn increment_created_orders(&self, trade_place_account: &TradePlaceAccount) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .increment_created_orders();
    }

    pub(crate) fn increment_canceled_orders(&self, trade_place_account: &TradePlaceAccount) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .increment_canceled_orders();
    }

    pub(crate) fn increment_partially_filled_orders(
        &self,
        trade_place_account: &TradePlaceAccount,
    ) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .increment_partially_filled_orders();
    }

    pub(crate) fn increment_completely_filled_orders(
        &self,
        trade_place_account: &TradePlaceAccount,
    ) {
        self.trade_place_stats
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .increment_completely_filled_orders();
    }

    pub(crate) fn add_summary_filled_amount(
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
    partially_filled_orders: Mutex<Vec<ClientOrderId>>,
}

impl StatisticEventHandler {
    pub fn new(events_receiver: broadcast::Receiver<ExchangeEvent>) -> Arc<Self> {
        let statistic_event_handler = Arc::new(Self {
            stats: StatisticService::default(),
            partially_filled_orders: Mutex::new(Vec::new()),
        });

        let action = statistic_event_handler.clone().start(events_receiver);
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
                        self.stats.increment_created_orders(&trade_place_account);
                    }
                    OrderEventType::CancelOrderSucceeded => {
                        self.stats.increment_canceled_orders(&trade_place_account);
                    }
                    OrderEventType::OrderFilled { cloned_order } => {
                        let client_order_id = &cloned_order.header.client_order_id;
                        let mut partially_filled_orders = self.partially_filled_orders.lock();

                        if !(*partially_filled_orders).contains(&client_order_id) {
                            self.stats
                                .increment_partially_filled_orders(&trade_place_account);
                            partially_filled_orders.push(client_order_id.clone());
                        }
                    }
                    OrderEventType::OrderCompleted { cloned_order } => {
                        let mut partially_filled_orders = self.partially_filled_orders.lock();
                        let client_order_id = &cloned_order.header.client_order_id;
                        if let Some(order_id_index) =
                            (*partially_filled_orders)
                                .iter()
                                .position(|stored_client_order_id| {
                                    stored_client_order_id == client_order_id
                                })
                        {
                            partially_filled_orders.swap_remove(order_id_index);
                        }

                        self.stats
                            .increment_completely_filled_orders(&trade_place_account);

                        let filled_amount = cloned_order.fills.filled_amount;
                        self.stats
                            .add_summary_filled_amount(&trade_place_account, filled_amount);

                        let commission = cloned_order
                            .fills
                            .fills
                            .iter()
                            .map(|fill| fill.commission_amount())
                            .sum();
                        self.stats
                            .add_summary_commission(&trade_place_account, commission);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub(crate) fn event_missed(&self) {
        self.stats.event_missed();
    }
}
