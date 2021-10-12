use std::sync::Arc;

use parking_lot::Mutex;

use crate::core::{
    balance_manager::balance_manager::BalanceManager,
    exchanges::{
        common::{Amount, TradePlaceAccount},
        exchange_blocker::{BlockReason, BlockType, ExchangeBlocker},
    },
    lifecycle::cancellation_token::CancellationToken,
    misc::position_helper,
    services::usd_converter::usd_converter::UsdConverter,
};

use super::balance_change_usd_periodic_calculator::BalanceChangeUsdPeriodicCalculator;

pub(crate) struct ProfitLossStopper {
    limit: Amount,
    target_trade_place: TradePlaceAccount,
    usd_periodic_calculator: BalanceChangeUsdPeriodicCalculator,
    exchange_blocker: Arc<ExchangeBlocker>,
    balance_manager: Arc<Mutex<BalanceManager>>,
    //TODO: fix me
    // private readonly IBotApi _botApi;
}

impl ProfitLossStopper {
    pub fn new(
        limit: Amount,
        target_trade_place: TradePlaceAccount,
        usd_periodic_calculator: BalanceChangeUsdPeriodicCalculator,
        exchange_blocker: Arc<ExchangeBlocker>,
        balance_manager: Arc<Mutex<BalanceManager>>,
        //TODO: fix me
        //         private readonly IBotApi _botApi;
    ) -> Self {
        Self {
            limit,
            target_trade_place,
            usd_periodic_calculator,
            exchange_blocker,
            balance_manager,
            //TODO: fix me
            //         private readonly IBotApi _botApi;
        }
    }

    pub async fn check_for_limit(
        &self,
        usd_converter: &UsdConverter,
        cancellation_token: CancellationToken,
    ) {
        let over_market = self
            .usd_periodic_calculator
            .calculate_over_market_usd_change(usd_converter, cancellation_token)
            .await;
        self.check(over_market).await;
    }

    async fn check(&self, usd_change: Amount) {
        let period = self.usd_periodic_calculator.period();

        log::info!(
            "ProfitLossStopper:check() {}: {} (limit {})",
            period,
            usd_change,
            self.limit
        );

        let target_exchange_account_id = self.target_trade_place.exchange_account_id.clone();

        if usd_change <= -self.limit {
            position_helper::close_position_if_needed(
                &self.target_trade_place,
                self.balance_manager.clone(),
            )
            .await; // REVIEW: await here is correct?

            if self
                .exchange_blocker
                .is_blocked_by_reason(&target_exchange_account_id, Self::block_reason())
            {
                return;
            }

            log::warn!(
                "Usd change for {}: {} exceeded {}",
                period,
                usd_change,
                self.limit
            );

            self.exchange_blocker.block(
                &target_exchange_account_id,
                Self::block_reason(),
                BlockType::Manual,
            );
        } else {
            if !self
                .exchange_blocker
                .is_blocked_by_reason(&target_exchange_account_id, Self::block_reason())
            {
                return;
            }

            log::warn!(
                "Usd change not {}: {} exceeded {}",
                period,
                usd_change,
                self.limit
            );

            self.exchange_blocker
                .unblock(&target_exchange_account_id, Self::block_reason());
        }
    }

    pub fn block_reason() -> BlockReason {
        BlockReason::new("ProfitLossExceeded")
    }
}
