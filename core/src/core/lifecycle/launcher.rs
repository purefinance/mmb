use super::trading_engine::Service;
use crate::core::balance_manager::balance_manager::BalanceManager;
use crate::core::config::{load_pretty_settings, try_load_settings};
use crate::core::exchanges::common::{ExchangeAccountId, ExchangeId};
use crate::core::exchanges::events::{ExchangeEvent, ExchangeEvents, CHANNEL_MAX_EVENTS_COUNT};
use crate::core::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::general::exchange_creation::create_exchange;
use crate::core::exchanges::general::exchange_creation::create_timeout_manager;
use crate::core::exchanges::internal_events_loop::InternalEventsLoop;
use crate::core::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::core::exchanges::traits::ExchangeClientBuilder;
use crate::core::lifecycle::application_manager::ApplicationManager;
use crate::core::lifecycle::trading_engine::{EngineContext, TradingEngine};
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::settings::{AppSettings, BaseStrategySettings, CoreSettings};
use crate::core::statistic_service::StatisticEventHandler;
use crate::core::statistic_service::StatisticService;
use crate::core::{
    disposition_execution::executor::DispositionExecutorService,
    infrastructure::{keep_application_manager, spawn_future},
};
use crate::rpc::control_panel::ControlPanel;
use crate::strategies::disposition_strategy::DispositionStrategy;
use anyhow::{anyhow, Result};
use core::fmt::Debug;
use dashmap::DashMap;
use futures::{future::join_all, FutureExt};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::hashmap;
use mmb_utils::logger::init_logger;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::convert::identity;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::{broadcast, mpsc, oneshot};

pub struct EngineBuildConfig {
    pub supported_exchange_clients: HashMap<ExchangeId, Box<dyn ExchangeClientBuilder + 'static>>,
}

impl EngineBuildConfig {
    pub fn standard(client_builder: Box<dyn ExchangeClientBuilder>) -> Self {
        let exchange_name = "Binance".into();
        let supported_exchange_clients = hashmap![exchange_name => client_builder];

        EngineBuildConfig {
            supported_exchange_clients,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum InitSettings<StrategySettings>
where
    StrategySettings: BaseStrategySettings + Clone,
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
) -> AppSettings<StrategySettings>
where
    StrategySettings: BaseStrategySettings + Clone + Debug + DeserializeOwned + Serialize,
{
    let (wait_config_tx, mut wait_config_rx) = mpsc::channel::<()>(10);

    let control_panel = ControlPanel::create_and_start_no_config(wait_config_tx)
        .expect("Failed to start control_panel without config");

    loop {
        match try_load_settings::<StrategySettings>(&config_path, &credentials_path) {
            Ok(settings) => {
                if let Some(mut rx_stop) = control_panel.graceful_shutdown() {
                    // extra time for stopping control_panel
                    tokio::time::sleep(Duration::from_secs(1)).await;

                    if let Err(error) = rx_stop.try_recv() {
                        log::warn!(
                            "Failed to receive stop signal from control panel: {:?}",
                            error
                        );
                    }
                }
                return settings;
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
    broadcast::Sender<ExchangeEvent>,
    broadcast::Receiver<ExchangeEvent>,
    AppSettings<StrategySettings>,
    DashMap<ExchangeAccountId, Arc<Exchange>>,
    Arc<EngineContext>,
    oneshot::Receiver<()>,
)>
where
    StrategySettings: BaseStrategySettings + Clone + Debug + DeserializeOwned + Serialize,
{
    init_logger();

    log::info!("*****************************");
    log::info!("TradingEngine starting");

    let settings = match init_user_settings {
        InitSettings::Directly(v) => v,
        InitSettings::Load {
            config_path,
            credentials_path,
        } => load_settings_or_wait::<StrategySettings>(&config_path, &credentials_path).await,
    };

    let application_manager = ApplicationManager::new(CancellationToken::new());
    keep_application_manager(application_manager.clone());
    let (events_sender, events_receiver) = broadcast::channel(CHANNEL_MAX_EVENTS_COUNT);

    let timeout_manager = create_timeout_manager(&settings.core, &build_settings);

    let exchanges = create_exchanges(
        &settings.core,
        build_settings,
        events_sender.clone(),
        application_manager.clone(),
        &timeout_manager,
    )
    .await;

    let exchanges_map: DashMap<_, _> = exchanges
        .into_iter()
        .map(|exchange| (exchange.exchange_account_id, exchange))
        .collect();

    let exchange_events = ExchangeEvents::new(events_sender.clone());

    let exchanges_hashmap: HashMap<ExchangeAccountId, Arc<Exchange>> =
        exchanges_map.clone().into_iter().collect();

    let currency_pair_to_symbol_converter = CurrencyPairToSymbolConverter::new(exchanges_hashmap);

    let balance_manager = BalanceManager::new(currency_pair_to_symbol_converter);

    BalanceManager::update_balances_for_exchanges(
        balance_manager.clone(),
        application_manager.stop_token(),
    )
    .await;

    for exchange in &exchanges_map {
        exchange
            .value()
            .setup_balance_manager(balance_manager.clone())
    }

    let (finish_graceful_shutdown_tx, finish_graceful_shutdown_rx) = oneshot::channel();
    let engine_context = EngineContext::new(
        settings.core.clone(),
        exchanges_map.clone(),
        exchange_events,
        finish_graceful_shutdown_tx,
        timeout_manager,
        application_manager.clone(),
        balance_manager,
    );

    Ok((
        events_sender,
        events_receiver,
        settings,
        exchanges_map,
        engine_context,
        finish_graceful_shutdown_rx,
    ))
}

fn run_services<'a, StrategySettings>(
    engine_context: Arc<EngineContext>,
    events_sender: broadcast::Sender<ExchangeEvent>,
    events_receiver: broadcast::Receiver<ExchangeEvent>,
    settings: AppSettings<StrategySettings>,
    exchanges_map: DashMap<ExchangeAccountId, Arc<Exchange>>,
    init_user_settings: InitSettings<StrategySettings>,
    build_strategy: impl Fn(
        &AppSettings<StrategySettings>,
        Arc<EngineContext>,
    ) -> Box<dyn DispositionStrategy + 'static>,
    finish_graceful_shutdown_rx: oneshot::Receiver<()>,
) -> TradingEngine
where
    StrategySettings: BaseStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    let internal_events_loop = InternalEventsLoop::new();
    engine_context
        .shutdown_service
        .register_service(internal_events_loop.clone());

    let exchange_events = ExchangeEvents::new(events_sender.clone());
    let statistic_service = StatisticService::new();
    let statistic_event_handler =
        create_statistic_event_handler(exchange_events, statistic_service.clone());
    let control_panel = ControlPanel::create_and_start(
        engine_context.application_manager.clone(),
        load_pretty_settings(init_user_settings),
        statistic_service.clone(),
    )
    .expect("Unable to start control panel");
    engine_context
        .shutdown_service
        .register_service(control_panel.clone());

    {
        let local_exchanges_map = exchanges_map.into_iter().map(identity).collect();
        let action = internal_events_loop.clone().start(
            events_receiver,
            local_exchanges_map,
            engine_context.application_manager.stop_token(),
        );
        let _ = spawn_future("internal_events_loop start", true, action.boxed());
    }

    let disposition_strategy = build_strategy(&settings, engine_context.clone());
    let disposition_executor_service = create_disposition_executor_service(
        &settings.strategy,
        &engine_context,
        disposition_strategy,
        &statistic_event_handler.stats,
    );
    engine_context
        .shutdown_service
        .register_service(disposition_executor_service);

    log::info!("TradingEngine started");
    TradingEngine::new(engine_context.clone(), finish_graceful_shutdown_rx)
}

pub(crate) fn handle_panic(
    application_manager: Option<Arc<ApplicationManager>>,
    panic: Box<dyn Any + Send>,
    message_template: &str,
) {
    match panic.as_ref().downcast_ref::<String>().clone() {
        Some(panic_message) => log::error!("{}: {}", message_template, panic_message),
        None => log::error!("{} without readable message", message_template),
    }

    if let Some(application_manager) = application_manager {
        application_manager
            .spawn_graceful_shutdown("Panic during TradeingEngine creation".to_owned());
    }
}

pub(crate) fn unwrap_or_handle_panic<T>(
    action_outcome: Result<T, Box<dyn Any + Send>>,
    message_template: &str,
    application_manager: Option<Arc<ApplicationManager>>,
) -> Result<T> {
    action_outcome.map_err(|panic| {
        handle_panic(application_manager, panic, message_template);

        anyhow!(message_template.to_owned())
    })
}

pub async fn launch_trading_engine<StrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<StrategySettings>,
    build_strategy: impl Fn(
        &AppSettings<StrategySettings>,
        Arc<EngineContext>,
    ) -> Box<dyn DispositionStrategy + 'static>,
) -> Result<TradingEngine>
where
    StrategySettings: BaseStrategySettings + Clone + Debug + DeserializeOwned + Serialize,
{
    let action_outcome = AssertUnwindSafe(before_engine_context_init(
        build_settings,
        init_user_settings.clone(),
    ))
    .catch_unwind()
    .await;

    let message_template = "Panic happened during EngineContext initialization";
    let (
        events_sender,
        events_receiver,
        settings,
        exchanges_map,
        engine_context,
        finish_graceful_shutdown_rx,
    ) = unwrap_or_handle_panic(action_outcome, message_template, None)??;

    let cloned_application_manager = engine_context.application_manager.clone();
    let action = async move {
        signal::ctrl_c().await.expect("failed to listen for event");

        log::info!("Ctrl-C signal was received so graceful_shutdown started");
        cloned_application_manager.spawn_graceful_shutdown("Ctrl-C signal was received".to_owned());

        Ok(())
    };

    let _ = spawn_future("Start Ctrl-C handler", true, action.boxed());

    let action_outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        run_services(
            engine_context.clone(),
            events_sender,
            events_receiver,
            settings,
            exchanges_map,
            init_user_settings,
            build_strategy,
            finish_graceful_shutdown_rx,
        )
    }));

    let message_template = "Panic happened during TradingEngine creation";
    unwrap_or_handle_panic(
        action_outcome,
        message_template,
        Some(engine_context.application_manager.clone()),
    )
}

fn create_disposition_executor_service(
    base_settings: &dyn BaseStrategySettings,
    engine_context: &Arc<EngineContext>,
    disposition_strategy: Box<dyn DispositionStrategy>,
    statistics: &Arc<StatisticService>,
) -> Arc<DispositionExecutorService> {
    DispositionExecutorService::new(
        engine_context.clone(),
        engine_context.get_events_channel(),
        LocalSnapshotsService::default(),
        base_settings.exchange_account_id(),
        base_settings.currency_pair(),
        base_settings.max_amount(),
        disposition_strategy,
        engine_context.application_manager.stop_token(),
        statistics.clone(),
    )
}

fn create_statistic_event_handler(
    events: ExchangeEvents,
    statistic_service: Arc<StatisticService>,
) -> Arc<StatisticEventHandler> {
    StatisticEventHandler::new(events.get_events_channel(), statistic_service)
}

pub async fn create_exchanges(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
    events_channel: broadcast::Sender<ExchangeEvent>,
    application_manager: Arc<ApplicationManager>,
    timeout_manager: &Arc<TimeoutManager>,
) -> Vec<Arc<Exchange>> {
    join_all(core_settings.exchanges.iter().map(|x| {
        create_exchange(
            x,
            build_settings,
            events_channel.clone(),
            application_manager.clone(),
            timeout_manager.clone(),
        )
    }))
    .await
}
