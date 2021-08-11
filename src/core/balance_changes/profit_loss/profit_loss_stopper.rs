use std::sync::Arc;

use log::{trace, warn};
use rust_decimal::Decimal;

use crate::core::balance_changes::balance_manager::balance_manager::BalanceManager;
use crate::core::balance_changes::calculator::BalanceChangePeriodicCalculator;
use crate::core::exchanges::block_reasons;
use crate::core::exchanges::common::TradePlaceAccount;
use crate::core::exchanges::exchange_blocker::BlockType;
use crate::core::exchanges::exchange_blocker::ExchangeBlocker;
use crate::core::misc::position_helper::PositionHelper;
pub struct ProfitLossStopper {
    limit: Decimal,
    periodic_calculator: BalanceChangePeriodicCalculator,
    target_trade_place: TradePlaceAccount,
    exchange_blocker: Arc<ExchangeBlocker>,
    balance_manager: BalanceManager,
}

impl ProfitLossStopper {
    pub fn new(
        limit: Decimal,
        periodic_calculator: BalanceChangePeriodicCalculator,
        target_trade_place: TradePlaceAccount,
        exchange_blocker: Arc<ExchangeBlocker>,
        balance_manager: BalanceManager,
    ) -> ProfitLossStopper {
        ProfitLossStopper {
            limit,
            periodic_calculator,
            target_trade_place,
            exchange_blocker,
            balance_manager,
        }
    }

    fn Check(&self, usb_cahnge: Decimal) {
        let period = self.periodic_calculator.period;

        trace!(
            "ProfitLossStopper check period = {:?} seconds: usb_cahnge = {} (limit {})",
            period,
            usb_cahnge,
            self.limit
        );

        let target_exchange_account_id = self.target_trade_place.exchange_account_id.clone();

        if usb_cahnge <= -self.limit {
            PositionHelper::close_position_if_needed(
                self.target_trade_place.clone(),
                self.balance_manager.clone(),
            );

            if self.exchange_blocker.is_blocked_by_reason(
                &target_exchange_account_id,
                block_reasons::PROFIT_LOSS_EXCEEDED,
            ) {
                ()
            }
            warn!(
                "Usd change for {:?}: {} exceeded {}",
                period, usb_cahnge, self.limit
            );

            self.exchange_blocker.block(
                &target_exchange_account_id,
                block_reasons::PROFIT_LOSS_EXCEEDED,
                BlockType::Manual,
            );
        } else {
            if !self.exchange_blocker.is_blocked_by_reason(
                &target_exchange_account_id,
                block_reasons::PROFIT_LOSS_EXCEEDED,
            ) {
                ()
            }
            warn!(
                "Usd change for {:?}: {} not exceeded {}",
                period, usb_cahnge, self.limit
            );
            self.exchange_blocker.unblock(
                &target_exchange_account_id,
                block_reasons::PROFIT_LOSS_EXCEEDED,
            );
        }
    }
}
