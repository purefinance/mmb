use std::sync::Arc;

use chrono::Duration;
use futures::future::join_all;
use itertools::Itertools;
use parking_lot::Mutex;

use crate::core::{
    balance_changes::{
        profit_balance_changes_calculator, profit_loss_balance_change::ProfitLossBalanceChange,
    },
    balance_manager::balance_manager::BalanceManager,
    exchanges::common::{Amount, TradePlaceAccount},
    lifecycle::cancellation_token::CancellationToken,
    services::usd_converter::usd_converter::UsdConverter,
};

use super::balance_change_period_selector::BalanceChangePeriodSelector;

pub(crate) struct BalanceChangeUsdPeriodicCalculator {
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

    pub fn add_balance_change(self: Arc<Self>, balance_change: &ProfitLossBalanceChange) {
        self.balance_change_period_selector
            .lock()
            .add(balance_change);
    }

    // TODO: fix when DatabaseManager will be added
    pub async fn load_data(
        self: Arc<Self>,
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

    pub fn calculate_raw_usd_change(&self, trade_place: &TradePlaceAccount) -> Amount {
        let items = self
            .balance_change_period_selector
            .lock()
            .get_items_by_trade_place(trade_place);
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
