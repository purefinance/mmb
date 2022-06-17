use async_trait::async_trait;

use mmb_utils::cancellation_token::CancellationToken;

use super::profit_loss_balance_change::ProfitLossBalanceChange;

#[async_trait]
pub(crate) trait BalanceChangeAccumulator {
    fn add_balance_change(&self, balance_change: &ProfitLossBalanceChange);

    // TODO: fix me when DatabaseManager will be implemented
    async fn load_data(
        &self,
        // database_manage: DatabaseManager,
        cancellation_token: CancellationToken,
    );
}
