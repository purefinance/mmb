use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use chrono::Duration;
use itertools::Itertools;
use mmb_utils::DateTime;
use mockall_double::double;
use parking_lot::Mutex;

use crate::balance::changes::profit_loss_balance_change::ProfitLossBalanceChange;
#[double]
use crate::balance::manager::balance_manager::BalanceManager;
use crate::balance::manager::position_change::PositionChange;
#[double]
use crate::misc::time::time_manager;
use domain::market::MarketAccountId;

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
            .entry(balance_change.market_account_id)
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
                log::error!("Can't find queue for trade place {market_account_id:?}");
                None
            })?;

        let position_change_before_period = match &self.balance_manager {
            Some(balance_manager) => {
                let position_change = balance_manager
                    .lock()
                    .get_last_position_change_before_period(market_account_id, start_of_period);

                log::info!("Balance changes list {start_of_period} {position_change:?}");

                position_change
            }
            None => {
                // if balance_manager isn't set we don't need to filter position_changes for web_server
                log::info!("Balance changes list {start_of_period} position_change is None");
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

#[cfg(test)]
mod tests {
    use mmb_utils::infrastructure::WithExpect;
    use rust_decimal_macros::dec;

    use crate::balance::changes::profit_loss_stopper::test::{
        create_balance_change, create_balance_change_by_market_account_id, market_account_id,
    };
    use crate::misc::time;
    use domain::market::{CurrencyPair, ExchangeAccountId, ExchangeId};
    use domain::order::snapshot::ClientOrderFillId;

    use super::*;

    #[test]
    fn test_position_change_before_period() {
        let (mut balance_manager, _locker) = BalanceManager::init_mock();

        let seconds_offset_in_mock = Arc::new(Mutex::new(0u32));
        let (_time_manager_context, _tm_locker) = time::tests::init_mock(seconds_offset_in_mock);
        let expect_position_change = Some(PositionChange::new(
            ClientOrderFillId::new("client_order_id_test".into()),
            time_manager::now() - Duration::minutes(30),
            dec!(1),
        ));

        let cloned_expect_position_change = expect_position_change.clone();
        balance_manager
            .expect_get_last_position_change_before_period()
            .returning(move |_, _| cloned_expect_position_change.clone())
            .times(6);

        let balance_manager = Arc::new(Mutex::new(balance_manager));
        let period_selector =
            BalanceChangePeriodSelector::new(Duration::hours(1), Some(balance_manager));

        for i in 0..5 {
            let balance_change = create_balance_change(
                dec!(1),
                time_manager::now() + Duration::minutes(10 * i),
                ClientOrderFillId::new(i.to_string().into()),
            );

            period_selector.lock().add(&balance_change);
        }

        let position_change = period_selector
            .lock()
            .synchronize_period(time_manager::now(), &market_account_id());

        assert_eq!(expect_position_change, position_change);
    }

    #[test]
    fn test_get_items() {
        let (mut balance_manager, _locker) = BalanceManager::init_mock();

        let seconds_offset_in_mock = Arc::new(Mutex::new(0u32));
        let (_time_manager_context, _tm_locker) = time::tests::init_mock(seconds_offset_in_mock);

        balance_manager
            .expect_get_last_position_change_before_period()
            .returning(move |_, _| None)
            .times(30);

        let balance_manager = Arc::new(Mutex::new(balance_manager));
        let period_selector =
            BalanceChangePeriodSelector::new(Duration::hours(1), Some(balance_manager));

        let expect = (0..5)
            .map(|i| {
                (0..5)
                    .map(|_| {
                        let market_account_id = MarketAccountId::new(
                            ExchangeAccountId::new(ExchangeId::new("exchange_test_id"), i),
                            CurrencyPair::from_codes("BTC".into(), "ETH".into()),
                        );
                        let balance_change = create_balance_change_by_market_account_id(
                            dec!(1),
                            time_manager::now() + Duration::minutes(10 * i as i64),
                            ClientOrderFillId::new(i.to_string().into()),
                            market_account_id,
                        );

                        period_selector.lock().add(&balance_change);

                        balance_change
                    })
                    .collect_vec()
            })
            .collect_vec();

        let mut result = period_selector.lock().get_items();
        result.sort();

        for (i, balance_change_vec) in result.into_iter().enumerate() {
            pretty_assertions::assert_eq!(
                expect
                    .get(i)
                    .with_expect(|| format!("Failed to get vec with number {i}")),
                &balance_change_vec
            );
        }
    }
}
