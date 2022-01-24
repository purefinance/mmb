use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use chrono::Duration;
use itertools::Itertools;
use mmb_utils::DateTime;
use mockall_double::double;
use parking_lot::Mutex;

#[double]
use crate::balance_manager::balance_manager::BalanceManager;
#[double]
use crate::misc::time::time_manager;

use crate::{
    balance_changes::profit_loss_balance_change::ProfitLossBalanceChange,
    balance_manager::position_change::PositionChange, exchanges::common::MarketAccountId,
};

pub(crate) struct BalanceChangePeriodSelector {
    pub(super) period: Duration,
    balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    balance_changes_queues: HashMap<MarketAccountId, VecDeque<ProfitLossBalanceChange>>,
}

impl BalanceChangePeriodSelector {
    pub fn new(
        period: Duration,
        balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            period,
            balance_manager,
            balance_changes_queues: HashMap::new(),
        }))
    }

    pub fn add(&mut self, balance_change: &ProfitLossBalanceChange) {
        log::info!(
            "Balance changes enqueue: {} {} {}",
            balance_change.change_date,
            balance_change.currency_code,
            balance_change.balance_change
        );

        self.balance_changes_queues
            .entry(balance_change.market_account_id.clone())
            .or_default()
            .push_back(balance_change.clone());

        self.synchronize_period(
            balance_change.change_date,
            &balance_change.market_account_id,
        );
    }

    fn synchronize_period(
        &mut self,
        now: DateTime,
        market_account_id: &MarketAccountId,
    ) -> Option<PositionChange> {
        let start_of_period = now - self.period;

        let balance_changes_queue = self
            .balance_changes_queues
            .get_mut(market_account_id)
            .or_else(|| {
                log::error!("Can't find queue for trade place {:?}", market_account_id);
                return None;
            })?;

        let position_change_before_period = match &self.balance_manager {
            Some(balance_manager) => {
                let position_change = balance_manager
                    .lock()
                    .get_last_position_change_before_period(market_account_id, start_of_period);

                log::info!(
                    "Balance changes list {} {:?}",
                    start_of_period,
                    position_change
                );

                position_change
            }
            None => {
                // if balance_manager isn't set we don't need to filter position_changes for web_server
                log::info!(
                    "Balance changes list {} position_change is None",
                    start_of_period,
                );
                None
            }
        };

        while let Some(last_change) = balance_changes_queue.front() {
            let should_skip_item = match position_change_before_period {
                Some(ref change) => last_change.client_order_fill_id == change.client_order_fill_id,
                None => last_change.change_date >= start_of_period,
            };

            if should_skip_item {
                break;
            }

            log::info!(
                "Balance changes dequeue {} {} {}",
                last_change.change_date,
                last_change.currency_code,
                last_change.balance_change
            );
            let _ = balance_changes_queue.pop_front();
        }
        position_change_before_period
    }

    pub fn get_items(&mut self) -> Vec<Vec<ProfitLossBalanceChange>> {
        self.balance_changes_queues
            .clone()
            .keys()
            .map(|current_market_account_id| {
                self.get_items_by_market_account_id(current_market_account_id)
            })
            .collect_vec()
    }

    pub fn get_items_by_market_account_id(
        &mut self,
        market_account_id: &MarketAccountId,
    ) -> Vec<ProfitLossBalanceChange> {
        let position_change = self.synchronize_period(time_manager::now(), market_account_id);

        let balance_changes_queue = self
            .balance_changes_queues
            .get(market_account_id)
            .expect("Failed to get balance changes queue by market_account_id");

        balance_changes_queue
            .iter()
            .map(|x| {
                if let Some(position_change) = &position_change {
                    if x.client_order_fill_id == position_change.client_order_fill_id {
                        return x.with_portion(position_change.portion);
                    }
                }
                x.clone()
            })
            .collect_vec()
    }
}
