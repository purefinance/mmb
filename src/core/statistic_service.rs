use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use super::exchanges::common::{Amount, Price, TradePlaceAccount};

// FIXME Probably it has to be pub(crate)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TradePlaceAccountStatistic {
    opened_orders_amount: usize,
    canceled_orders_amount: usize,
    partially_filled_orders_amount: usize,
    fully_filled_orders_amount: usize,
    summary_filled_amount: Amount,
    summary_fee: Price,
}

impl TradePlaceAccountStatistic {
    fn new(
        opened_orders_amount: usize,
        canceled_orders_amount: usize,
        partially_filled_orders_amount: usize,
        fully_filled_orders_amount: usize,
        summary_filled_amount: Amount,
        summary_fee: Price,
    ) -> Self {
        Self {
            opened_orders_amount,
            canceled_orders_amount,
            partially_filled_orders_amount,
            fully_filled_orders_amount,
            summary_filled_amount,
            summary_fee,
        }
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

// FIXME in what meaning should it be Service? Should it be able to call graceful shutdown?
#[derive(Debug, Serialize, Deserialize)]
pub struct StatisticService {
    trade_place_data: RwLock<HashMap<TradePlaceAccount, TradePlaceAccountStatistic>>,
    disposition_executor_data: Mutex<DispositionExecutorStatistic>,
}

impl StatisticService {
    // FIXME Probably it has to be pub(crate)
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            trade_place_data: Default::default(),
            disposition_executor_data: Default::default(),
        })
    }

    pub(crate) fn order_created(self: Arc<Self>, trade_place_account: TradePlaceAccount) {
        // TODO get_or_add logic
        self.trade_place_data
            .write()
            .insert(trade_place_account, TradePlaceAccountStatistic::default());
        dbg!(&"ORDER CREATED");
    }

    pub(crate) fn order_canceled(self: Arc<Self>, trade_place_account: TradePlaceAccount) {
        dbg!(&"ORDER CANCELED");
    }

    pub(crate) fn order_partially_filled(self: Arc<Self>, trade_place_account: TradePlaceAccount) {
        dbg!(&"ORDER PARTIALLY FILLED");
    }

    pub(crate) fn order_completely_filled(self: Arc<Self>, trade_place_account: TradePlaceAccount) {
        // FIXME delete order from partially filled
        dbg!(&"ORDER PARTIALLY FILLED");
    }

    // FIXME add summary fillled amount and summary commission

    pub(crate) fn event_missed(self: Arc<Self>) {
        (*self.disposition_executor_data.lock()).skipped_events_amount += 1;
    }
}
