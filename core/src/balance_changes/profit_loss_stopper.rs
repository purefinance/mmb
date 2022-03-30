use std::sync::Arc;

use mmb_utils::cancellation_token::CancellationToken;
use mockall_double::double;
use parking_lot::Mutex;

#[double]
use crate::balance_manager::balance_manager::BalanceManager;
#[double]
use crate::exchanges::exchange_blocker::ExchangeBlocker;
#[double]
use crate::exchanges::general::engine_api::EngineApi;
#[double]
use crate::services::usd_convertion::usd_converter::UsdConverter;

use crate::{
    exchanges::{
        common::{Amount, MarketAccountId},
        exchange_blocker::{BlockReason, BlockType},
    },
    misc::position_helper,
};

use super::balance_change_usd_periodic_calculator::BalanceChangeUsdPeriodicCalculator;

static BLOCK_REASON: BlockReason = BlockReason::new("ProfitLossExceeded");

pub(crate) struct ProfitLossStopper {
    limit: Amount,
    target_market_account_id: MarketAccountId,
    usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
    exchange_blocker: Arc<ExchangeBlocker>,
    balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    engine_api: Arc<EngineApi>,
}

impl ProfitLossStopper {
    pub fn new(
        limit: Amount,
        target_market_account_id: MarketAccountId,
        usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
        exchange_blocker: Arc<ExchangeBlocker>,
        balance_manager: Option<Arc<Mutex<BalanceManager>>>,
        engine_api: Arc<EngineApi>,
    ) -> Self {
        Self {
            limit,
            target_market_account_id,
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
            "ProfitLossStopper::check() {}: {} (limit {})",
            period,
            usd_change,
            self.limit
        );

        let target_exchange_account_id = self.target_market_account_id.exchange_account_id;

        if usd_change <= -self.limit {
            let _ = position_helper::close_position_if_needed(
                &self.target_market_account_id,
                self.balance_manager.clone(),
                self.engine_api.clone(),
                cancellation_token,
            );

            if self
                .exchange_blocker
                .is_blocked_by_reason(target_exchange_account_id, BLOCK_REASON)
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
                target_exchange_account_id,
                BLOCK_REASON,
                BlockType::Manual,
            );
        } else {
            if !self
                .exchange_blocker
                .is_blocked_by_reason(target_exchange_account_id, BLOCK_REASON)
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
                .unblock(target_exchange_account_id, BLOCK_REASON);
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    use std::sync::Arc;

    use chrono::Duration;
    use mmb_utils::{logger::init_logger_file_named, DateTime};
    use parking_lot::{Mutex, ReentrantMutexGuard};
    use rust_decimal_macros::dec;

    #[double]
    use crate::misc::time::time_manager;
    use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;
    use crate::{
        balance_changes::{
            balance_change_usd_periodic_calculator::BalanceChangeUsdPeriodicCalculator,
            balance_changes_accumulator::BalanceChangeAccumulator,
            profit_loss_balance_change::{ProfitLossBalanceChange, ProfitLossBalanceChangeId},
        },
        balance_manager::position_change::PositionChange,
        exchanges::common::{
            Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, ExchangeId, MarketAccountId,
        },
        misc::time,
        orders::order::ClientOrderFillId,
    };

    use super::ProfitLossStopper;

    fn exchange_id() -> ExchangeId {
        ExchangeId::new("exchange_test_id".into())
    }

    fn exchange_account_id() -> ExchangeAccountId {
        ExchangeAccountId::new(exchange_id(), 0)
    }

    fn currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes(btc(), "ETH".into())
    }

    pub(crate) fn market_account_id() -> MarketAccountId {
        MarketAccountId::new(exchange_account_id(), currency_pair())
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
        pub balance_change_usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
        pub profit_loss_stopper: ProfitLossStopper,
        pub usd_converter: UsdConverter,
        pub balance_manager: Arc<Mutex<BalanceManager>>,

        _exchange_blocker: Arc<ExchangeBlocker>,
        _time_manager_mock: time_manager::__now::Context,
        seconds_offset_in_mock: Arc<Mutex<u32>>,
        _mock_lockers: Vec<ReentrantMutexGuard<'static, ()>>,
    }

    impl TestContext {
        pub fn new(
            _exchange_blocker: Arc<ExchangeBlocker>,
            balance_change_usd_periodic_calculator: Arc<BalanceChangeUsdPeriodicCalculator>,
            profit_loss_stopper: ProfitLossStopper,
            usd_converter: UsdConverter,
            balance_manager: Arc<Mutex<BalanceManager>>,
            _time_manager_mock: time_manager::__now::Context,
            seconds_offset_in_mock: Arc<Mutex<u32>>,
            _mock_lockers: Vec<ReentrantMutexGuard<'static, ()>>,
        ) -> Self {
            Self {
                balance_change_usd_periodic_calculator,
                profit_loss_stopper,
                usd_converter,
                balance_manager,
                _exchange_blocker,
                _time_manager_mock,
                seconds_offset_in_mock,
                _mock_lockers,
            }
        }
    }

    fn init(max_period: Duration, get_last_position_change_calling_times: usize) -> TestContext {
        let (exchange_blocker, exchange_blocker_locker) = ExchangeBlocker::init_mock();
        init_with_exchange_blocker(
            max_period,
            Arc::new(exchange_blocker),
            exchange_blocker_locker,
            get_last_position_change_calling_times,
        )
    }

    fn init_with_exchange_blocker(
        max_period: Duration,
        exchange_blocker: Arc<ExchangeBlocker>,
        exchange_blocker_locker: ReentrantMutexGuard<'static, ()>,
        get_last_position_change_calling_times: usize,
    ) -> TestContext {
        let seconds_offset_in_mock = Arc::new(Mutex::new(0u32));
        let mut mock_lockers = vec![exchange_blocker_locker];
        let (time_manager_mock, time_manager_mock_locker) =
            time::tests::init_mock(seconds_offset_in_mock.clone());
        mock_lockers.push(time_manager_mock_locker);

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

        let (exchange, exchange_locker) = EngineApi::init_mock();
        mock_lockers.push(exchange_locker);
        let exchange = Arc::new(exchange);

        let profit_loss_stopper = ProfitLossStopper::new(
            LIMIT,
            market_account_id(),
            balance_change_usd_periodic_calculator.clone(),
            exchange_blocker.clone(),
            Some(balance_manager.clone()),
            exchange,
        );

        let (mut usd_converter, usd_converter_locker) = UsdConverter::init_mock();
        mock_lockers.push(usd_converter_locker);
        usd_converter
            .expect_convert_amount()
            .returning(|_, b, _| Some(dec!(0.5) * b));

        TestContext::new(
            exchange_blocker,
            balance_change_usd_periodic_calculator,
            profit_loss_stopper,
            usd_converter,
            balance_manager,
            time_manager_mock,
            seconds_offset_in_mock,
            mock_lockers,
        )
    }

    pub(crate) fn create_balance_change(
        usd_balance_change: Amount,
        change_date: DateTime,
        client_order_fill_id: ClientOrderFillId,
    ) -> ProfitLossBalanceChange {
        create_balance_change_by_market_account_id(
            usd_balance_change,
            change_date,
            client_order_fill_id,
            market_account_id(),
        )
    }

    pub(crate) fn create_balance_change_by_market_account_id(
        usd_balance_change: Amount,
        change_date: DateTime,
        client_order_fill_id: ClientOrderFillId,
        market_account_id: MarketAccountId,
    ) -> ProfitLossBalanceChange {
        ProfitLossBalanceChange {
            id: ProfitLossBalanceChangeId::generate(),
            client_order_fill_id,
            change_date,
            configuration_descriptor: ConfigurationDescriptor {
                service_name: "test".into(),
                service_configuration_key: "test".into(),
            },
            exchange_id: exchange_id(),
            market_account_id,
            currency_code: btc(),
            balance_change: usd_balance_change * dec!(2),
            usd_price: dec!(1),
            usd_balance_change: usd_balance_change * dec!(2),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn add_change_should_calculate_usd_correctly() {
        init_logger_file_named("log.txt");
        let context = init(max_period(), 4);

        context
            .balance_change_usd_periodic_calculator
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn add_change_should_ignore_old_data() {
        init_logger_file_named("log.txt");
        let context = init(max_period(), 3);

        context
            .balance_change_usd_periodic_calculator
            .add_balance_change(&create_balance_change(
                dec!(1),
                time_manager::now() - (max_period() + Duration::seconds(1)),
                ClientOrderFillId::unique_id(),
            ));

        context
            .balance_change_usd_periodic_calculator
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn check_for_limit_should_stop_transaction() {
        init_logger_file_named("log.txt");
        let (mut exchange_blocker, exchange_blocker_locker) = ExchangeBlocker::init_mock();
        exchange_blocker
            .expect_block()
            .returning(|_, _, _| ())
            .times(1);

        exchange_blocker
            .expect_is_blocked_by_reason()
            .returning(|_, _| false);

        let context = init_with_exchange_blocker(
            max_period(),
            Arc::new(exchange_blocker),
            exchange_blocker_locker,
            4,
        );

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn check_for_limit_should_recover_after_positive_tarde() {
        init_logger_file_named("log.txt");
        let (mut exchange_blocker, exchange_blocker_locker) = ExchangeBlocker::init_mock();
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

        let context = init_with_exchange_blocker(
            max_period(),
            Arc::new(exchange_blocker),
            exchange_blocker_locker,
            6,
        );

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn check_for_limit_should_recover_after_first_change_expired() {
        init_logger_file_named("log.txt");
        let (mut exchange_blocker, exchange_blocker_locker) = ExchangeBlocker::init_mock();
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

        let context = init_with_exchange_blocker(
            Duration::seconds(3),
            Arc::new(exchange_blocker),
            exchange_blocker_locker,
            4,
        );

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    pub async fn check_for_limit_should_recover_after_time_period() {
        init_logger_file_named("log.txt");
        let (mut exchange_blocker, exchange_blocker_locker) = ExchangeBlocker::init_mock();
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

        let context = init_with_exchange_blocker(
            Duration::seconds(3),
            Arc::new(exchange_blocker),
            exchange_blocker_locker,
            4,
        );

        context
            .balance_manager
            .lock()
            .expect_get_position()
            .returning(|_, _, _| dec!(0));

        context
            .balance_change_usd_periodic_calculator
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
