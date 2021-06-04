use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use futures::future::join_all;
use itertools::Itertools;
use log::info;
use tokio::sync::oneshot;

use crate::core::exchanges::application_manager::ApplicationManager;
use crate::core::exchanges::block_reasons;
use crate::core::exchanges::cancellation_token::CancellationToken;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::events::ExchangeEvents;
use crate::core::exchanges::exchange_blocker::BlockType;
use crate::core::exchanges::exchange_blocker::ExchangeBlocker;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::core::lifecycle::shutdown::ShutdownService;
use crate::core::settings::CoreSettings;

pub trait Service: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>>;
}

pub struct EngineContext {
    pub app_settings: CoreSettings,
    pub exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
    pub shutdown_service: Arc<ShutdownService>,
    pub exchange_blocker: Arc<ExchangeBlocker>,
    pub application_manager: Arc<ApplicationManager>,
    pub timeout_manager: Arc<TimeoutManager>,
    is_graceful_shutdown_started: AtomicBool,
    exchange_events: ExchangeEvents,
}

impl EngineContext {
    pub(crate) fn new(app_settings: CoreSettings, exchange_events: ExchangeEvents) -> Arc<Self> {
        let exchange_account_ids = app_settings
            .exchanges
            .iter()
            .map(|x| x.exchange_account_id.clone())
            .collect_vec();

        let application_manager = ApplicationManager::new(CancellationToken::new());

        let timeout_manager = TimeoutManager::new();

        let engine_context = Arc::new(EngineContext {
            app_settings,
            exchanges: Default::default(),
            shutdown_service: Default::default(),
            exchange_blocker: ExchangeBlocker::new(exchange_account_ids),
            application_manager: application_manager.clone(),
            timeout_manager,
            is_graceful_shutdown_started: Default::default(),
            exchange_events,
        });

        application_manager.setup_engine_context(engine_context.clone());

        engine_context
    }

    pub(crate) async fn graceful_shutdown(&self) {
        if self
            .is_graceful_shutdown_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        info!("Graceful shutdown started");

        self.exchanges.iter().for_each(|x| {
            self.exchange_blocker.block(
                &x.exchange_account_id,
                block_reasons::GRACEFUL_SHUTDOWN,
                BlockType::Manual,
            )
        });

        self.application_manager.stop_token().cancel();

        self.shutdown_service.graceful_shutdown().await;
        self.exchange_blocker.stop_blocker().await;
        cancel_opened_orders(&self.exchanges).await;

        info!("Graceful shutdown finished");
    }
}

async fn cancel_opened_orders(exchanges: &DashMap<ExchangeAccountId, Arc<Exchange>>) {
    info!("Canceling opened orders started");

    join_all(exchanges.iter().map(|x| x.clone().cancel_opened_orders())).await;

    info!("Canceling opened orders finished");
}
