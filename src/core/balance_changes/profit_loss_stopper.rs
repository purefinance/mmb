use std::sync::Arc;

use mockall_double::double;
use parking_lot::Mutex;

#[double]
use crate::core::balance_manager::balance_manager::BalanceManager;
#[double]
use crate::core::exchanges::exchange_blocker::ExchangeBlocker;
#[double]
use crate::core::exchanges::general::engine_api::EngineApi;
#[double]
use crate::core::services::usd_converter::usd_converter::UsdConverter;

use crate::core::{
    exchanges::{
        common::{Amount, TradePlaceAccount},
        exchange_blocker::{BlockReason, BlockType},
    },
    lifecycle::cancellation_token::CancellationToken,
    misc::position_helper,
};

use super::balance_change_usd_periodic_calculator::BalanceChangeUsdPeriodicCalculator;

pub(crate) struct ProfitLossStopper {
    limit: Amount,
    target_trade_place: TradePlaceAccount,
    usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
    exchange_blocker: Arc<ExchangeBlocker>,
    balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    engine_api: Arc<EngineApi>,
}

impl ProfitLossStopper {
    pub fn new(
        limit: Amount,
        target_trade_place: TradePlaceAccount,
        usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
        exchange_blocker: Arc<ExchangeBlocker>,
        balance_manager: Option<Arc<Mutex<BalanceManager>>>,
        engine_api: Arc<EngineApi>,
    ) -> Self {
        Self {
            limit,
            target_trade_place,
            usd_periodic_calculator,
            exchange_blocker,
            balance_manager,
            engine_api,
        }
    }

    pub async fn check_for_limit(
        &self,
        usd_converter: &UsdConverter,
        cancellation_token: CancellationToken,
    ) {
        let over_market = self
            .usd_periodic_calculator
            .calculate_over_market_usd_change(usd_converter, cancellation_token.clone())
            .await;
        self.check(over_market, cancellation_token).await;
    }

    async fn check(&self, usd_change: Amount, cancellation_token: CancellationToken) {
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
                self.engine_api.clone(),
                cancellation_token,
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

#[cfg(test)]
mod test {
    use super::*;

    use std::sync::Arc;

    use chrono::{Duration, TimeZone};
    use parking_lot::Mutex;
    use rust_decimal_macros::dec;

    #[double]
    use crate::core::misc::time_manager::time_manager;
    use crate::core::{
        balance_changes::{
            balance_change_usd_periodic_calculator::BalanceChangeUsdPeriodicCalculator,
            profit_loss_balance_change::{ProfitLossBalanceChange, ProfitLossBalanceChangeId},
        },
        balance_manager::position_change::PositionChange,
        exchanges::common::{
            Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, ExchangeId, TradePlaceAccount,
        },
        logger::init_logger,
        orders::order::ClientOrderFillId,
        DateTime,
    };

    use super::ProfitLossStopper;

    fn exchange_id() -> ExchangeId {
        ExchangeId::new("exchange_test_id".into())
    }

    fn exchange_account_id() -> ExchangeAccountId {
        ExchangeAccountId::new(exchange_id(), 0)
    }

    fn currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes(&btc(), &"ETH".into())
    }

    fn trade_place() -> TradePlaceAccount {
        TradePlaceAccount::new(exchange_account_id(), currency_pair())
    }

    fn btc() -> CurrencyCode {
        "BTC".into()
    }

    static LIMIT: Amount = dec!(10);

    fn max_period() -> Duration {
        Duration::hours(1)
    }

    fn client_order_fill_id() -> ClientOrderFillId {
        ClientOrderFillId::new("client_order_id_test".into())
    }

    struct TestContext {
        pub exchange_blocker: Arc<ExchangeBlocker>,
        pub balance_change_usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
        pub profit_loss_stopper: ProfitLossStopper,
        pub usd_converter: UsdConverter,
        pub balance_manager: Arc<Mutex<BalanceManager>>,

        time_manager_mock_object: time_manager::__now::Context,
        seconds_offset_in_mock: Arc<Mutex<u32>>,
    }

    impl TestContext {
        pub fn new(
            exchange_blocker: Arc<ExchangeBlocker>,
            balance_change_usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
            profit_loss_stopper: ProfitLossStopper,
            usd_converter: UsdConverter,
            balance_manager: Arc<Mutex<BalanceManager>>,

            time_manager_mock_object: time_manager::__now::Context,
            seconds_offset_in_mock: Arc<Mutex<u32>>,
        ) -> Self {
            Self {
                exchange_blocker,
                balance_change_usd_periodic_calculator,
                profit_loss_stopper,
                usd_converter,
                balance_manager,
                time_manager_mock_object,
                seconds_offset_in_mock,
            }
        }
    }

    fn init(max_period: Duration, get_last_position_change_calling_times: usize) -> TestContext {
        let exchange_blocker = Arc::new(ExchangeBlocker::default());
        init_with_exchange_blocker(
            max_period,
            exchange_blocker,
            get_last_position_change_calling_times,
        )
    }

    fn init_with_exchange_blocker(
        max_period: Duration,
        exchange_blocker: Arc<ExchangeBlocker>,
        get_last_position_change_calling_times: usize,
    ) -> TestContext {
        let seconds_offset_in_mock = Arc::new(Mutex::new(0u32));
        let time_manager_mock_object = time_manager::now_context();
        let seconds = seconds_offset_in_mock.clone();
        time_manager_mock_object.expect().returning(move || {
            chrono::Utc
                .ymd(2021, 9, 20)
                .and_hms(0, 0, seconds.lock().clone())
        });

        let balance_manager = Arc::new(Mutex::new(BalanceManager::default()));
        balance_manager
            .lock()
            .expect_get_last_position_change_before_period()
            .returning(|_, _| {
                Some(PositionChange::new(
                    client_order_fill_id(),
                    time_manager::now(),
                    dec!(1),
                ))
            })
            .times(get_last_position_change_calling_times);

        let balance_change_usd_periodic_calculator =
            BalanceChangeUsdPeriodicCalculator::new(max_period, Some(balance_manager.clone()));

        let exchange = Arc::new(EngineApi::default());

        let profit_loss_stopper = ProfitLossStopper::new(
            LIMIT,
            trade_place(),
            balance_change_usd_periodic_calculator.clone(),
            exchange_blocker.clone(),
            Some(balance_manager.clone()),
            exchange,
        );

        let mut usd_converter = UsdConverter::default();
        usd_converter
            .expect_convert_amount()
            .returning(|_, b, _| Some(dec!(0.5) * b));

        TestContext::new(
            exchange_blocker,
            balance_change_usd_periodic_calculator,
            profit_loss_stopper,
            usd_converter,
            balance_manager,
            time_manager_mock_object,
            seconds_offset_in_mock,
        )
    }

    fn create_balance_change(
        usd_balance_change: Amount,
        change_date: DateTime,
        client_order_fill_id: ClientOrderFillId,
    ) -> ProfitLossBalanceChange {
        ProfitLossBalanceChange {
            id: ProfitLossBalanceChangeId::generate(),
            client_order_fill_id,
            change_date: change_date,
            service_name: "test".to_string(),
            service_configuration_key: "test".to_string(),
            exchange_id: exchange_id(),
            trade_place: TradePlaceAccount::new(exchange_account_id(), currency_pair()),
            currency_code: btc(),
            balance_change: usd_balance_change * dec!(2),
            usd_price: dec!(1),
            usd_balance_change: usd_balance_change * dec!(2),
        }
    }

    #[tokio::test]
    pub async fn add_change_should_calculate_usd_correctly() {
        let context = init(max_period(), 4);

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(2),
                time_manager::now(),
                client_order_fill_id(),
            ));

        let over_market_usd_change_1 = context
            .balance_change_usd_periodic_calculator
            .calculate_over_market_usd_change(&context.usd_converter, CancellationToken::default())
            .await;
        assert_eq!(over_market_usd_change_1, dec!(2));

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(3),
                time_manager::now(),
                client_order_fill_id(),
            ));

        let over_market_usd_change_2 = context
            .balance_change_usd_periodic_calculator
            .calculate_over_market_usd_change(&context.usd_converter, CancellationToken::default())
            .await;
        assert_eq!(over_market_usd_change_2, dec!(2) + dec!(3));
    }

    #[tokio::test]
    pub async fn add_change_should_ignore_old_data() {
        let context = init(max_period(), 3);

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(1),
                time_manager::now() - (max_period() + Duration::seconds(1)),
                ClientOrderFillId::unique_id(),
            ));

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(2),
                time_manager::now(),
                client_order_fill_id(),
            ));

        let over_market_usd_change = context
            .balance_change_usd_periodic_calculator
            .calculate_over_market_usd_change(&context.usd_converter, CancellationToken::default())
            .await;
        assert_eq!(over_market_usd_change, dec!(2));
    }

    #[tokio::test]
    pub async fn check_for_limit_should_stop_transaction() {
        let mut exchange_blocker = ExchangeBlocker::default();
        exchange_blocker
            .expect_block()
            .returning(|_, _, _| ())
            .times(1);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| false);

        let context = init_with_exchange_blocker(max_period(), Arc::new(exchange_blocker), 4);

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-8),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-3),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;
    }

    #[tokio::test]
    pub async fn check_for_limit_should_recover_after_positive_tarde() {
        let mut exchange_blocker = ExchangeBlocker::default();
        exchange_blocker
            .expect_block()
            .returning(|_, _, _| ())
            .times(1);

        exchange_blocker
            .expect_unblock()
            .returning(|_, _| ())
            .times(1);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| false)
            .times(2);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| true)
            .times(1);

        let context = init_with_exchange_blocker(max_period(), Arc::new(exchange_blocker), 6);

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-8),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-3),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(2),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;
    }

    #[tokio::test]
    pub async fn check_for_limit_should_recover_after_first_change_expired() {
        init_logger();
        let mut exchange_blocker = ExchangeBlocker::default();
        exchange_blocker
            .expect_block()
            .returning(|_, _, _| ())
            .times(1);

        exchange_blocker
            .expect_unblock()
            .returning(|_, _| ())
            .times(1);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| false)
            .times(2);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| true)
            .times(1);

        let context =
            init_with_exchange_blocker(Duration::seconds(3), Arc::new(exchange_blocker), 4);

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-8),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-3),
                time_manager::now(),
                ClientOrderFillId::new(
                    "needed_to_simulate_that_the_first_change_has_expired".into(),
                ),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        context
            .balance_manager
            .lock()
            .expect_get_last_position_change_before_period()
            .returning(|_, _| {
                Some(PositionChange::new(
                    ClientOrderFillId::new(
                        "needed_to_simulate_that_the_first_change_has_expired".into(),
                    ),
                    time_manager::now(),
                    dec!(1),
                ))
            });

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;
    }

    #[tokio::test]
    pub async fn check_for_limit_should_recover_after_time_period() {
        init_logger();
        let mut exchange_blocker = ExchangeBlocker::default();
        exchange_blocker
            .expect_block()
            .returning(|_, _, _| ())
            .times(1);

        exchange_blocker
            .expect_unblock()
            .returning(|_, _| ())
            .times(1);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| false)
            .times(2);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| true)
            .times(1);

        let context =
            init_with_exchange_blocker(Duration::seconds(3), Arc::new(exchange_blocker), 4);

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-8),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        *context.seconds_offset_in_mock.lock() = 2;

        context
            .balance_change_usd_periodic_calculator
            .clone()
            .add_balance_change(&create_balance_change(
                dec!(-3),
                time_manager::now(),
                client_order_fill_id(),
            ));

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;

        context
            .balance_manager
            .lock()
            .expect_get_last_position_change_before_period()
            .returning(|_, _| None);

        *context.seconds_offset_in_mock.lock() = 4;

        context
            .profit_loss_stopper
            .check_for_limit(&context.usd_converter, CancellationToken::default())
            .await;
    }
}
