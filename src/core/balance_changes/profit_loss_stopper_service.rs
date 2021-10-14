use std::sync::Arc;

use chrono::Duration;
use futures::future::join_all;
use parking_lot::Mutex;

use crate::core::{
    balance_manager::balance_manager::BalanceManager,
    exchanges::{
        common::TradePlaceAccount, exchange_blocker::ExchangeBlocker, general::exchange::Exchange,
    },
    lifecycle::cancellation_token::CancellationToken,
    services::usd_converter::usd_converter::UsdConverter,
    settings::{ProfitLossStopperSettings, TimePeriodKind},
};

use super::{
    balance_change_usd_periodic_calculator::BalanceChangeUsdPeriodicCalculator,
    profit_loss_balance_change::ProfitLossBalanceChange, profit_loss_stopper::ProfitLossStopper,
};

pub(crate) struct ProfitLossStopperService {
    target_trade_place: TradePlaceAccount,
    exchange_blocker: Arc<ExchangeBlocker>,
    exchange: Arc<Exchange>,

    profit_loss_stoppers: Vec<ProfitLossStopper>,
    usd_periodic_calculators: Vec<Arc<BalanceChangeUsdPeriodicCalculator>>,
}

impl ProfitLossStopperService {
    pub fn new(
        target_trade_place: TradePlaceAccount,
        stopper_settings: ProfitLossStopperSettings,
        exchange_blocker: Arc<ExchangeBlocker>,
        balance_manager: Option<Arc<Mutex<BalanceManager>>>,
        exchange: Arc<Exchange>,
    ) -> Self {
        let mut this = Self {
            target_trade_place,
            exchange_blocker,
            exchange,
            profit_loss_stoppers: Vec::new(),
            usd_periodic_calculators: Vec::new(),
        };

        Self::validate_settings(&stopper_settings);
        this.create_stoppers(stopper_settings, balance_manager);

        this
    }

    fn create_stoppers(
        &mut self,
        stopper_settings: ProfitLossStopperSettings,
        balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    ) {
        for stopper_condition in stopper_settings.conditions {
            let period = match stopper_condition.period_kind {
                TimePeriodKind::Hour => Duration::hours(stopper_condition.period_value),
                TimePeriodKind::Day => Duration::days(stopper_condition.period_value),
            };
            let usd_periodic_calculator =
                BalanceChangeUsdPeriodicCalculator::new(period, balance_manager.clone());
            let profit_loss_stopper = ProfitLossStopper::new(
                stopper_condition.limit,
                self.target_trade_place.clone(),
                usd_periodic_calculator.clone(),
                self.exchange_blocker.clone(),
                balance_manager.clone(),
                self.exchange.clone(),
            );

            self.usd_periodic_calculators.push(usd_periodic_calculator);
            self.profit_loss_stoppers.push(profit_loss_stopper);
        }
    }

    pub fn add_balance_change(&mut self, balance_change: &ProfitLossBalanceChange) {
        for usd_periodic_calculator in self.usd_periodic_calculators.iter() {
            usd_periodic_calculator
                .clone()
                .add_balance_change(balance_change);
        }
    }

    // TODO: Fix me when DatabaseManager will be implemented
    pub async fn load_data(
        &self,
        // database_manager: DatabaseManager,
        cancellation_token: CancellationToken,
    ) {
        let futures = self.usd_periodic_calculators.iter().map(|x| {
            x.clone().load_data(
                // database_manager: DatabaseManager,
                cancellation_token.clone(),
            )
        });

        join_all(futures).await;
    }

    pub fn get_periodic_calculators(&self) -> &Vec<Arc<BalanceChangeUsdPeriodicCalculator>> {
        &self.usd_periodic_calculators
    }

    fn validate_settings(stopper_settings: &ProfitLossStopperSettings) {
        if stopper_settings.conditions.is_empty() {
            panic!("ProfitLossStopperService::validate_settings() stopper_settings.conditions shouldn't be empty.")
        }
    }

    pub async fn check_for_limit(
        &self,
        usd_converter: &UsdConverter,
        cancellation_token: CancellationToken,
    ) {
        let futures = self
            .profit_loss_stoppers
            .iter()
            .map(|x| x.check_for_limit(usd_converter, cancellation_token.clone()));

        join_all(futures).await;
    }
}

#[cfg(test)]
mod test {
    use rust_decimal_macros::dec;

    use crate::core::{
        exchanges::{
            common::{CurrencyPair, ExchangeAccountId, TradePlaceAccount},
            general::test_helper::get_test_exchange,
        },
        settings::StopperCondition,
    };

    use super::*;

    fn exchange_account_id() -> ExchangeAccountId {
        ExchangeAccountId::new("exchange_test_id".into(), 0)
    }

    fn trade_place() -> TradePlaceAccount {
        TradePlaceAccount::new(
            exchange_account_id(),
            CurrencyPair::from_codes(&"BTC".into(), &"ETH".into()),
        )
    }

    #[tokio::test]
    pub async fn settings_loading_test_empty_settings_should_not_throw() {
        let stopper_settings = ProfitLossStopperSettings {
            conditions: vec![StopperCondition {
                period_kind: TimePeriodKind::Day,
                period_value: 1,
                limit: dec!(50),
            }],
        };
        ProfitLossStopperService::new(
            trade_place(),
            stopper_settings,
            ExchangeBlocker::new(vec![exchange_account_id()]),
            None,
            get_test_exchange(false).0,
        );
    }
}
