use anyhow::Result;
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
    opened_orders_amount: usize,
    canceled_orders_amount: usize,
    partially_filled_orders_amount: usize,
    fully_filled_orders_amount: usize,
    summary_filled_amount: Amount,
    summary_commission: Price,
}

impl TradePlaceAccountStatistic {
    fn order_created(&mut self) {
        self.opened_orders_amount += 1;
    }

    fn order_canceled(&mut self) {
        self.canceled_orders_amount += 1;
    }

    fn order_partially_filled(&mut self) {
        self.partially_filled_orders_amount += 1;
    }

    fn order_completely_filled(&mut self) {
        self.partially_filled_orders_amount = self.partially_filled_orders_amount.saturating_sub(1);
        self.fully_filled_orders_amount += 1;
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

pub struct StatisticEventHandler {
    stats: StatisticService,
    events_receiver: broadcast::Receiver<ExchangeEvent>,
}

impl StatisticEventHandler {
    pub fn new(events_receiver: broadcast::Receiver<ExchangeEvent>) -> Arc<Self> {
        let statistic_event_handler = Arc::new(Self {
            stats: StatisticService::default(),
            events_receiver,
        });

        let cloned_self = statistic_event_handler.clone();
        let action = async move { cloned_self.clone().start().await };
        spawn_future("Start statistic service", true, action.boxed());

        statistic_event_handler
    }

    pub async fn start(&self) -> Result<()> {
        //let mut trading_context: Option<TradingContext> = None;

        //loop {
        //    let event = tokio::select! {
        //        event_res = self.events_receiver.recv() => event_res.context("Error during receiving event in DispositionExecutor::start()")?,
        //        _ = self.cancellation_token.when_cancelled() => {
        //            let _ = self.work_finished_sender.take().ok_or(anyhow!("Can't take `work_finished_sender` in DispositionExecutor"))?.send(Ok(()));
        //            return Ok(());
        //        }
        //    };

        //    self.handle_event(event, &mut trading_context)?;
        //}
        todo!()
    }

    pub(crate) fn event_missed(self: Arc<Self>) {
        // FIXME implement
        todo!()
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct StatisticService {
    trade_place_data: RwLock<HashMap<TradePlaceAccount, TradePlaceAccountStatistic>>,
    disposition_executor_data: Mutex<DispositionExecutorStatistic>,
}

impl StatisticService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            trade_place_data: Default::default(),
            disposition_executor_data: Default::default(),
        })
    }

    pub(crate) fn order_created(self: Arc<Self>, trade_place_account: &TradePlaceAccount) {
        self.trade_place_data
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_created();
    }

    pub(crate) fn order_canceled(self: Arc<Self>, trade_place_account: &TradePlaceAccount) {
        self.trade_place_data
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_canceled();
    }

    pub(crate) fn order_partially_filled(self: Arc<Self>, trade_place_account: &TradePlaceAccount) {
        self.trade_place_data
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_partially_filled();
    }

    pub(crate) fn order_completely_filled(
        self: Arc<Self>,
        trade_place_account: &TradePlaceAccount,
    ) {
        self.trade_place_data
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .order_completely_filled();
    }

    pub(crate) fn add_summary_amount(
        self: Arc<Self>,
        trade_place_account: &TradePlaceAccount,
        filled_amount: Amount,
    ) {
        self.trade_place_data
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .add_summary_filled_amount(filled_amount);
    }

    pub(crate) fn add_summary_commission(
        self: Arc<Self>,
        trade_place_account: &TradePlaceAccount,
        commission: Price,
    ) {
        self.trade_place_data
            .write()
            .entry(trade_place_account.clone())
            .or_default()
            .add_summary_commission(commission);
    }

    pub(crate) fn event_missed(self: Arc<Self>) {
        (*self.disposition_executor_data.lock()).skipped_events_amount += 1;
    }
}
