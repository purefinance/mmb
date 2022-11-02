use super::launcher::unwrap_or_handle_panic;
use crate::balance::manager::balance_manager::BalanceManager;
use crate::database::events::recorder::EventRecorder;
use crate::disposition_execution::executor::DispositionExecutorService;
use crate::disposition_execution::strategy::DispositionStrategy;
use crate::exchanges::block_reasons;
use crate::exchanges::exchange_blocker::BlockType;
use crate::exchanges::exchange_blocker::ExchangeBlocker;
use crate::exchanges::general::exchange::Exchange;
use crate::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::infrastructure::unset_lifetime_manager;
use crate::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;
use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use crate::lifecycle::shutdown::ShutdownService;
use crate::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::settings::DispositionStrategySettings;
use crate::settings::{AppSettings, CoreSettings};
use crate::statistic_service::{StatisticEventHandler, StatisticService};
use anyhow::Result;
use dashmap::DashMap;
use futures::future::join_all;
use futures::FutureExt;
use mmb_domain::events::{ExchangeEvent, ExchangeEvents};
use mmb_domain::market::ExchangeAccountId;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::logger::print_info;
use mmb_utils::nothing_to_do;
use mmb_utils::send_expected::SendExpected;
use parking_lot::Mutex;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};
use tokio::time::{timeout, Duration};

pub trait Service: Send + Sync + 'static {
    fn name(&self) -> &str;

    /// Execute graceful shutdown for current service
    /// Returns `Some(oneshot::Receiver)` that specified when service shutdowned or `None` if
    /// service already finished its work
    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>>;
}

pub struct EngineContext {
    pub core_settings: CoreSettings,
    pub exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
    pub shutdown_service: Arc<ShutdownService>,
    pub exchange_blocker: Arc<ExchangeBlocker>,
    pub lifetime_manager: Arc<AppLifetimeManager>,
    pub timeout_manager: Arc<TimeoutManager>,
    pub balance_manager: Arc<Mutex<BalanceManager>>,
    pub event_recorder: Arc<EventRecorder>,
    pub statistic_service: Arc<StatisticService>,
    is_graceful_shutdown_started: AtomicBool,
    exchange_events: ExchangeEvents,
    finish_graceful_shutdown_sender: Mutex<Option<oneshot::Sender<ActionAfterGracefulShutdown>>>,
}

impl EngineContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        core_settings: CoreSettings,
        exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
        exchange_events: ExchangeEvents,
        finish_graceful_shutdown_sender: oneshot::Sender<ActionAfterGracefulShutdown>,
        exchange_blocker: Arc<ExchangeBlocker>,
        timeout_manager: Arc<TimeoutManager>,
        lifetime_manager: Arc<AppLifetimeManager>,
        balance_manager: Arc<Mutex<BalanceManager>>,
        event_recorder: Arc<EventRecorder>,
    ) -> Arc<Self> {
        let statistic_service = StatisticService::new();
        let engine_context = Arc::new(EngineContext {
            core_settings,
            exchanges,
            shutdown_service: Default::default(),
            exchange_blocker,
            lifetime_manager: lifetime_manager.clone(),
            timeout_manager,
            balance_manager,
            event_recorder,
            statistic_service,
            is_graceful_shutdown_started: Default::default(),
            exchange_events,
            finish_graceful_shutdown_sender: Mutex::new(Some(finish_graceful_shutdown_sender)),
        });

        lifetime_manager.setup_engine_context(engine_context.clone());

        engine_context
    }

    pub(crate) async fn graceful_shutdown(
        self: Arc<Self>,
        action: ActionAfterGracefulShutdown,
        futures_cancellation_token: CancellationToken,
    ) {
        if self
            .is_graceful_shutdown_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        print_info("Graceful shutdown started");

        self.exchanges.iter().for_each(|x| {
            self.exchange_blocker.block(
                x.exchange_account_id,
                block_reasons::GRACEFUL_SHUTDOWN,
                BlockType::Manual,
            )
        });

        self.lifetime_manager.stop_token().cancel();

        self.shutdown_service.user_lvl_shutdown().await;
        self.exchange_blocker.stop_blocker().await;

        let cancellation_token = CancellationToken::default();
        const TIMEOUT: Duration = Duration::from_secs(5);

        match timeout(
            TIMEOUT,
            cancel_opened_orders(&self.exchanges, cancellation_token.clone(), true),
        )
        .await
        {
            Ok(()) => (),
            Err(_) => {
                cancellation_token.cancel();
                log::error!(
                    "Timeout {} secs is exceeded: cancel open orders has been stopped",
                    TIMEOUT.as_secs(),
                );
            }
        }

        match timeout(
            TIMEOUT,
            close_active_positions(&self.exchanges, cancellation_token.clone()),
        )
        .await
        {
            Ok(()) => (),
            Err(_) => {
                cancellation_token.cancel();
                log::error!(
                    "Timeout {} secs is exceeded: active positions closing has been stopped",
                    TIMEOUT.as_secs(),
                );
            }
        }

        self.shutdown_service.core_lvl_shutdown().await;

        match timeout(Duration::from_secs(5), self.event_recorder.flush_and_stop()).await {
            Err(_) => log::error!("In graceful shutdown EventRecorder::flush_and_stop() was not finished during 5 seconds"),
            Ok(Err(err)) => log::error!("In graceful shutdown error from EventRecorder::flush_and_stop(): {err:?}"),
            Ok(Ok(())) => nothing_to_do(),
        }

        let disconnect_websockets = self
            .exchanges
            .iter()
            .map(|exchange| async move { exchange.clone().disconnect_ws().await });
        join_all(disconnect_websockets).await;

        self.finish_graceful_shutdown_sender
            .lock()
            .take()
            .expect("'finish_graceful_shutdown_sender' should exists in EngineContext")
            .send_expected(action);

        if let ActionAfterGracefulShutdown::Restart = action {
            futures_cancellation_token.cancel();
        }

        unset_lifetime_manager();

        print_info("Graceful shutdown finished");
    }

    pub fn get_events_channel(&self) -> broadcast::Receiver<ExchangeEvent> {
        self.exchange_events.get_events_channel()
    }
}

async fn cancel_opened_orders(
    exchanges: &DashMap<ExchangeAccountId, Arc<Exchange>>,
    cancellation_token: CancellationToken,
    add_missing_open_orders: bool,
) {
    log::info!("Canceling opened orders started");

    join_all(exchanges.iter().map(|x| {
        x.clone()
            .cancel_opened_orders(cancellation_token.clone(), add_missing_open_orders)
    }))
    .await;

    log::info!("Canceling opened orders finished");
}

async fn close_active_positions(
    exchanges: &DashMap<ExchangeAccountId, Arc<Exchange>>,
    cancellation_token: CancellationToken,
) {
    log::info!("Closing active positions started");

    join_all(
        exchanges
            .iter()
            .filter(|x| x.exchange_client.get_settings().is_margin_trading)
            .map(|x| x.clone().close_active_positions(cancellation_token.clone())),
    )
    .await;

    log::info!("Closing active positions finished");
}

pub struct TradingEngine<StrategySettings: Clone> {
    context: Arc<EngineContext>,
    settings: AppSettings<StrategySettings>,
    finished_graceful_shutdown: oneshot::Receiver<ActionAfterGracefulShutdown>,
}

impl<StrategySettings: Clone> TradingEngine<StrategySettings> {
    pub fn new(
        context: Arc<EngineContext>,
        settings: AppSettings<StrategySettings>,
        finished_graceful_shutdown: oneshot::Receiver<ActionAfterGracefulShutdown>,
    ) -> Self {
        TradingEngine {
            context,
            settings,
            finished_graceful_shutdown,
        }
    }

    pub fn context(&self) -> Arc<EngineContext> {
        self.context.clone()
    }
    pub fn settings(&self) -> &AppSettings<StrategySettings> {
        &self.settings
    }

    pub async fn run(self) -> ActionAfterGracefulShutdown {
        join_all(self.context.exchanges.iter().map(|x| async move {
            x.value().connect_ws().await.with_expect(move || {
                "Failed to connect to websockets on exchange {exchange_account_id}"
            });
        }))
        .await;

        let action_outcome = AssertUnwindSafe(self.finished_graceful_shutdown)
            .catch_unwind()
            .await;

        unwrap_or_handle_panic(
            action_outcome,
            "Panic happened while TradingEngine was run",
            Some(self.context.lifetime_manager.clone()),
        )
        .expect("unwrap_or_handle_panic returned error")
        .expect("Failed to receive message from finished_graceful_shutdown")
    }

    /// Starts `DispositionExecutor` trading pattern assumes that orders will be placed
    /// on the exchange almost all the time
    pub fn start_disposition_executor(&self, strategy: Box<dyn DispositionStrategy>)
    where
        StrategySettings: DispositionStrategySettings,
    {
        let ctx = self.context();
        let settings = self.settings();

        let statistics =
            StatisticEventHandler::new(ctx.get_events_channel(), ctx.statistic_service.clone());

        let base_settings = &settings.strategy;
        let disposition_executor_service = DispositionExecutorService::new(
            ctx.clone(),
            ctx.get_events_channel(),
            LocalSnapshotsService::default(),
            base_settings.exchange_account_id(),
            base_settings.currency_pair(),
            strategy,
            ctx.lifetime_manager.stop_token(),
            statistics.stats.clone(),
        );

        ctx.shutdown_service
            .register_user_service(disposition_executor_service);
    }
}
