use std::sync::Arc;

use async_trait::async_trait;
use chrono::Duration;
use domain::market::MarketAccountId;
use domain::order::snapshot::Amount;
use futures::future::join_all;
use itertools::Itertools;
use mmb_utils::cancellation_token::CancellationToken;
use mockall_double::double;
use parking_lot::Mutex;

#[double]
use crate::balance::manager::balance_manager::BalanceManager;
#[double]
use crate::services::usd_convertion::usd_converter::UsdConverter;

use crate::balance::changes::{
    balance_changes_accumulator::BalanceChangeAccumulator, profit_balance_changes_calculator,
    profit_loss_balance_change::ProfitLossBalanceChange,
};

use super::balance_change_period_selector::BalanceChangePeriodSelector;

pub struct BalanceChangeUsdPeriodicCalculator {
    balance_change_period_selector: Arc<Mutex<BalanceChangePeriodSelector>>,
}

impl BalanceChangeUsdPeriodicCalculator {
    pub fn new(period: Duration, balance_manager: Option<Arc<Mutex<BalanceManager>>>) -> Arc<Self> {
        Arc::new(Self {
            balance_change_period_selector: BalanceChangePeriodSelector::new(
                period,
                balance_manager,
            ),
        })
    }

    pub fn calculate_raw_usd_change(&self, market_account_id: &MarketAccountId) -> Amount {
        let items = self
            .balance_change_period_selector
            .lock()
            .get_items_by_market_account_id(market_account_id);
        profit_balance_changes_calculator::calculate_raw(&items)
    }

    pub async fn calculate_over_market_usd_change(
        &self,
        usd_converter: &UsdConverter,
        cancellation_token: CancellationToken,
    ) -> Amount {
        let items = self.balance_change_period_selector.lock().get_items();

        let actions = items
            .iter()
            .map(|x| {
                profit_balance_changes_calculator::calculate_over_market(
                    x,
                    usd_converter,
                    cancellation_token.clone(),
                )
            })
            .collect_vec();

        join_all(actions).await.iter().sum()
    }

    pub fn period(&self) -> Duration {
        self.balance_change_period_selector.lock().period
    }
}

#[async_trait]
impl BalanceChangeAccumulator for BalanceChangeUsdPeriodicCalculator {
    // TODO: fix when DatabaseManager will be added
    async fn load_data(
        &self,
        // database_manager: DatabaseManager,
        _cancellation_token: CancellationToken,
    ) {
        //             await using var session = databaseManager.Sql;

        //             var fromDate = _dateTimeService.UtcNow - Period;
        //             var balanceChanges = await session.Set<ProfitLossBalanceChange>()
        //                 .Where(x => x.DateTime >= fromDate)
        //                 .OrderBy(x => x.DateTime)
        //                 .ToListAsync(cancellationToken);

        //             foreach (var balanceChange in balanceChanges)
        //             {
        //                 _balanceChangePeriodSelector.Add(balanceChange);
        //             }
    }

    fn add_balance_change(&self, balance_change: &ProfitLossBalanceChange) {
        self.balance_change_period_selector
            .lock()
            .add(balance_change);
    }
}
