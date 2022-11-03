use crate::balance::manager::balance_manager::BalanceManager;
use crate::config::{load_pretty_settings, try_load_settings};
use crate::database::events::recorder::EventRecorder;
use crate::exchanges::exchange_blocker::ExchangeBlocker;
use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
use crate::exchanges::general::exchange::Exchange;
use crate::exchanges::general::exchange_creation::create_exchange;
use crate::exchanges::general::exchange_creation::create_timeout_manager;
use crate::exchanges::internal_events_loop::InternalEventsLoop;
use crate::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::exchanges::traits::ExchangeClientBuilder;
use crate::infrastructure::spawn_future;
use crate::infrastructure::{init_lifetime_manager, spawn_by_timer, spawn_future_ok};
use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use crate::lifecycle::trading_engine::{EngineContext, TradingEngine};
use crate::rpc::config_waiter::ConfigWaiter;
use crate::rpc::core_api::CoreApi;
use crate::services::cleanup_orders::CleanupOrdersService;
use crate::settings::{AppSettings, CoreSettings, DispositionStrategySettings};
use anyhow::{anyhow, bail, Context, Result};
use core::fmt::Debug;
use dashmap::DashMap;
use futures::{future::join_all, FutureExt};
use itertools::Itertools;
use mmb_database::postgres_db::migrator::apply_migrations;
use mmb_database::postgres_db::PgPool;
use mmb_domain::events::{ExchangeEvent, ExchangeEvents, CHANNEL_MAX_EVENTS_COUNT};
use mmb_domain::market::ExchangeAccountId;
use mmb_domain::market::ExchangeId;
use mmb_utils::infrastructure::{init_infrastructure, SpawnFutureFlags};
use mmb_utils::logger::print_info;
use mmb_utils::nothing_to_do;
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::signal;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::timeout;
use uuid::Uuid;

use crate::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;
use crate::services::cleanup_database::CleanupDatabaseService;
use crate::services::live_ranges::LiveRangesService;

pub struct EngineBuildConfig {
    pub supported_exchange_clients: HashMap<ExchangeId, Box<dyn ExchangeClientBuilder + 'static>>,
}

impl EngineBuildConfig {
    pub fn new(client_builders: Vec<Box<dyn ExchangeClientBuilder>>) -> Self {
        let mut supported_exchange_clients = HashMap::new();
        for builder in client_builders {
            supported_exchange_clients.insert(builder.get_exchange_id(), builder);
        }

        EngineBuildConfig {
            supported_exchange_clients,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum InitSettings<StrategySettings>
where
    StrategySettings: DispositionStrategySettings + Clone,
{
    Directly(AppSettings<StrategySettings>),
    Load {
        config_path: String,
        credentials_path: String,
    },
}

pub async fn load_settings_or_wait<StrategySettings>(
    config_path: &str,
    credentials_path: &str,
) -> Option<AppSettings<StrategySettings>>
where
    StrategySettings: DispositionStrategySettings + Clone + Debug + DeserializeOwned + Serialize,
{
    let (wait_config_tx, mut wait_config_rx) = mpsc::channel::<()>(10);

    let wait_for_config = ConfigWaiter::create_and_start(wait_config_tx)
        .expect("Failed to start RPC server to waiting for config");

    let mut work_finished_receiver = wait_for_config
        .work_finished_receiver
        .lock()
        .take()
        .expect("work_finished_receiver is None");

    loop {
        if work_finished_receiver.try_recv().is_ok() {
            return None;
        }

        match try_load_settings::<StrategySettings>(config_path, credentials_path) {
            Ok(settings) => {
                wait_for_config.stop_server();

                match timeout(Duration::from_secs(3), work_finished_receiver).await {
                    Ok(_) => nothing_to_do(),
                    Err(_) => log::warn!("Failed to receive stop signal from ConfigWaiter"),
                }

                return Some(settings);
            }
            Err(error) => {
                log::trace!("Failed to load settings: {:?}", error);
                wait_config_rx.recv().await;
            }
        }
    }
}

async fn before_engine_context_init<StrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<StrategySettings>,
) -> Result<(
    broadcast::Receiver<ExchangeEvent>,
    AppSettings<StrategySettings>,
    DashMap<ExchangeAccountId, Arc<Exchange>>,
    Arc<EngineContext>,
    oneshot::Receiver<ActionAfterGracefulShutdown>,
    Option<PgPool>,
)>
where
    StrategySettings: DispositionStrategySettings + Clone + Debug + DeserializeOwned + Serialize,
{
    init_infrastructure();

    log::info!("*****************************");
    log::info!("TradingEngine starting");

    let lifetime_manager = init_lifetime_manager();

    let settings = match init_user_settings {
        InitSettings::Directly(v) => v,
        InitSettings::Load {
            config_path,
            credentials_path,
        } => {
            match load_settings_or_wait::<StrategySettings>(&config_path, &credentials_path).await {
                Some(settings) => settings,
                None => bail!("Error loading settings"),
            }
        }
    };

    let (events_sender, events_receiver) = broadcast::channel(CHANNEL_MAX_EVENTS_COUNT);

    let timeout_manager = create_timeout_manager(&settings.core, build_settings);

    let exchange_account_ids = settings
        .core
        .exchanges
        .iter()
        .map(|x| x.exchange_account_id)
        .collect_vec();

    let exchange_blocker = ExchangeBlocker::new(exchange_account_ids);

    let (pool, postponed_events_dir) = if let Some(db) = &settings.core.database {
        apply_migrations(&db.url, db.migrations.clone())
            .await
            .context("unable apply db migrations")?;

        let pool = PgPool::create(&db.url, 5)
            .await
            .with_context(|| format!("from `launcher` with connection_string: {}", &db.url))?;

        (Some(pool), db.postponed_events_dir.clone())
    } else {
        (None, None)
    };

    let event_recorder = EventRecorder::start(pool.clone(), postponed_events_dir)
        .await
        .expect("can't start EventRecorder");

    let exchanges = create_exchanges(
        &settings.core,
        build_settings,
        events_sender.clone(),
        lifetime_manager.clone(),
        &timeout_manager,
        Arc::downgrade(&exchange_blocker),
        event_recorder.clone(),
    )
    .await;

    let exchanges_map: DashMap<_, _> = exchanges
        .into_iter()
        .map(|exchange| (exchange.exchange_account_id, exchange))
        .collect();

    let exchanges_hashmap: HashMap<ExchangeAccountId, Arc<Exchange>> =
        exchanges_map.clone().into_iter().collect();

    let currency_pair_to_symbol_converter = CurrencyPairToSymbolConverter::new(exchanges_hashmap);

    let balance_manager = BalanceManager::new(
        currency_pair_to_symbol_converter,
        Some(event_recorder.clone()),
    );

    BalanceManager::update_balances_for_exchanges(
        balance_manager.clone(),
        lifetime_manager.stop_token(),
    )
    .await;

    for exchange in &exchanges_map {
        exchange
            .value()
            .setup_balance_manager(balance_manager.clone())
    }

    start_updating_balances(&lifetime_manager, &balance_manager);

    let (finish_graceful_shutdown_tx, finish_graceful_shutdown_rx) = oneshot::channel();

    let engine_context = EngineContext::new(
        settings.core.clone(),
        exchanges_map.clone(),
        ExchangeEvents::new(events_sender),
        finish_graceful_shutdown_tx,
        exchange_blocker,
        timeout_manager,
        lifetime_manager.clone(),
        balance_manager,
        event_recorder,
    );

    Ok((
        events_receiver,
        settings,
        exchanges_map,
        engine_context,
        finish_graceful_shutdown_rx,
        pool,
    ))
}

fn start_updating_balances(
    lifetime_manager: &Arc<AppLifetimeManager>,
    balance_manager: &Arc<Mutex<BalanceManager>>,
) {
    spawn_by_timer(
        "Update balances",
        Duration::from_secs(60),
        Duration::from_secs(60),
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        {
            let balance_manager = balance_manager.clone();
            let stop_token = lifetime_manager.stop_token();
            move || {
                BalanceManager::update_balances_for_exchanges(
                    balance_manager.clone(),
                    stop_token.clone(),
                )
            }
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn run_services<'a, StrategySettings>(
    engine_context: Arc<EngineContext>,
    events_receiver: broadcast::Receiver<ExchangeEvent>,
    settings: AppSettings<StrategySettings>,
    exchanges_map: DashMap<ExchangeAccountId, Arc<Exchange>>,
    init_user_settings: InitSettings<StrategySettings>,
    finish_graceful_shutdown_rx: oneshot::Receiver<ActionAfterGracefulShutdown>,
    cleanup_orders_service: Arc<CleanupOrdersService>,
    data_services: Option<DataServices>,
) -> TradingEngine<StrategySettings>
where
    StrategySettings: DispositionStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    let internal_events_loop = InternalEventsLoop::new();
    engine_context
        .shutdown_service
        .register_core_service(internal_events_loop.clone());

    let control_panel = CoreApi::create_and_start(
        engine_context.lifetime_manager.clone(),
        load_pretty_settings(init_user_settings),
        engine_context.statistic_service.clone(),
    )
    .expect("Unable to start control panel");
    engine_context
        .shutdown_service
        .register_core_service(control_panel);

    engine_context
        .shutdown_service
        .register_core_service(cleanup_orders_service.clone());

    let _ = spawn_future(
        "internal_events_loop start",
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        internal_events_loop.start(
            events_receiver,
            exchanges_map.into_iter().collect(),
            engine_context.lifetime_manager.stop_token(),
        ),
    );

    if let Some(data_services) = data_services {
        engine_context
            .shutdown_service
            .register_core_service(data_services.live_range_service.clone());

        let _ = spawn_by_timer(
            "live ranges",
            Duration::ZERO,
            Duration::from_secs(1),
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            move || data_services.live_range_service.clone().push(),
        );

        engine_context
            .shutdown_service
            .register_core_service(data_services.cleanup_database_service.clone());

        let _ = spawn_by_timer(
            "cleanup database",
            Duration::ZERO,
            Duration::from_secs(60 * 60), // one hour
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            move || data_services.cleanup_database_service.clone().run(),
        );
    }

    let _ = spawn_by_timer(
        "cleanup_outdated_orders",
        Duration::ZERO,
        Duration::from_secs(600), // 10 minutes
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        move || cleanup_orders_service.clone().cleanup_outdated_orders(),
    );

    log::info!("TradingEngine started");
    TradingEngine::new(engine_context, settings, finish_graceful_shutdown_rx)
}

pub(crate) fn unwrap_or_handle_panic<T>(
    action_outcome: Result<T, Box<dyn Any + Send>>,
    message_template: &'static str,
    lifetime_manager: Option<Arc<AppLifetimeManager>>,
) -> Result<T> {
    action_outcome.map_err(|err| {
        if let Some(lifetime_manager) = lifetime_manager {
            lifetime_manager.spawn_graceful_shutdown("Panic during TradingEngine creation");
        }

        enum ErrorMessage {
            String(String),
            ConstStr(&'static str),
            // Unable convert panic message to readable string
            None,
        }
        impl Display for ErrorMessage {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    ErrorMessage::String(ref str) => f.write_str(str),
                    ErrorMessage::ConstStr(str) => f.write_str(str),
                    ErrorMessage::None => f.write_str("Unable convert error to readable message"),
                }
            }
        }

        struct FullError(&'static str, ErrorMessage);
        impl Debug for FullError {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}: {}", self.0, self.1)
            }
        }

        impl Display for FullError {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{self:?}")
            }
        }

        let error_msg = match err.downcast::<String>() {
            Ok(msg) => ErrorMessage::String(*msg),
            Err(err) => match err.downcast::<&'static str>() {
                Ok(msg) => ErrorMessage::ConstStr(*msg),
                Err(_) => ErrorMessage::None,
            },
        };

        let full_error = FullError(message_template, error_msg);

        log::error!("{full_error}");

        anyhow!(full_error)
    })
}

pub struct DataServices {
    live_range_service: Arc<LiveRangesService>,
    cleanup_database_service: Arc<CleanupDatabaseService>,
}

pub async fn launch_trading_engine<StrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<StrategySettings>,
) -> Result<TradingEngine<StrategySettings>>
where
    StrategySettings: DispositionStrategySettings + Clone + Debug + DeserializeOwned + Serialize,
{
    print_info("The TradingEngine is going to start...");
    let action_outcome = AssertUnwindSafe(before_engine_context_init(
        build_settings,
        init_user_settings.clone(),
    ))
    .catch_unwind()
    .await;

    let message_template = "Panic happened during EngineContext initialization";
    let (
        events_receiver,
        settings,
        exchanges_map,
        engine_context,
        finish_graceful_shutdown_rx,
        pool,
    ) = unwrap_or_handle_panic(action_outcome, message_template, None)??;

    let cloned_lifetime_manager = engine_context.lifetime_manager.clone();
    let action = async move {
        signal::ctrl_c().await.expect("failed to listen for event");

        print_info("Ctrl-C signal was received so graceful_shutdown will be started");
        cloned_lifetime_manager.spawn_graceful_shutdown("Ctrl-C signal was received");
    };

    let _ = spawn_future_ok(
        "Start Ctrl-C handler",
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        action,
    );

    let cleanup_orders_service =
        Arc::new(CleanupOrdersService::new(engine_context.exchanges.clone()));

    let data_services = match pool {
        None => None,
        Some(pool) => {
            let session_id = Uuid::new_v4().to_string();
            let live_range_service = Arc::new(LiveRangesService::new(session_id, pool.clone()));
            let cleanup_database_service = Arc::new(CleanupDatabaseService::new(pool));
            Some(DataServices {
                live_range_service,
                cleanup_database_service,
            })
        }
    };

    let action_outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        run_services(
            engine_context.clone(),
            events_receiver,
            settings,
            exchanges_map,
            init_user_settings,
            finish_graceful_shutdown_rx,
            cleanup_orders_service,
            data_services,
        )
    }));

    let message_template = "Panic happened during TradingEngine creation";
    let result = unwrap_or_handle_panic(
        action_outcome,
        message_template,
        Some(engine_context.lifetime_manager.clone()),
    );

    print_info("The TradingEngine has been successfully launched");

    result
}

pub async fn create_exchanges(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
    events_channel: broadcast::Sender<ExchangeEvent>,
    lifetime_manager: Arc<AppLifetimeManager>,
    timeout_manager: &Arc<TimeoutManager>,
    exchange_blocker: Weak<ExchangeBlocker>,
    event_recorder: Arc<EventRecorder>,
) -> Vec<Arc<Exchange>> {
    join_all(core_settings.exchanges.iter().map(|x| {
        create_exchange(
            x,
            build_settings,
            events_channel.clone(),
            lifetime_manager.clone(),
            timeout_manager.clone(),
            exchange_blocker.clone(),
            event_recorder.clone(),
        )
    }))
    .await
}
