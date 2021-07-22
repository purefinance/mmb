use std::sync::Arc;

use dashmap::DashMap;

use super::exchanges::common::{Amount, Price, TradePlaceAccount};

// FIXME Probably it has to be pub(crate)
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

#[derive(Default)]
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
pub struct StatisticService {
    trade_place_data: DashMap<TradePlaceAccount, TradePlaceAccountStatistic>,
    disposition_executor_data: DispositionExecutorStatistic,
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
        dbg!(&"HERE");
    }
}
