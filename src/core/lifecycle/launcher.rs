use crate::core::exchanges::binance::binance::BinanceBuilder;
use crate::core::exchanges::common::ExchangeId;
use crate::core::exchanges::events::{ExchangeEvent, ExchangeEvents, CHANNEL_MAX_EVENTS_COUNT};
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::general::exchange_creation::create_exchange;
use crate::core::exchanges::general::exchange_creation::create_timeout_manager;
use crate::core::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::core::exchanges::traits::ExchangeClientBuilder;
use crate::core::internal_events_loop::InternalEventsLoop;
use crate::core::lifecycle::application_manager::ApplicationManager;
use crate::core::lifecycle::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::{EngineContext, TradingEngine};
use crate::core::logger::init_logger;
use crate::core::order_book::local_snapshot_service::LocalSnapshotsService;
use crate::core::settings::{AppSettings, BaseStrategySettings, CoreSettings};
use crate::core::{config::load_settings, statistic_service::StatisticService};
use crate::core::{
    disposition_execution::executor::DispositionExecutorService,
    infrastructure::{keep_application_manager, spawn_future},
};
use crate::hashmap;
use crate::rest_api::control_panel::ControlPanel;
use crate::strategies::disposition_strategy::DispositionStrategy;
use anyhow::Result;
use core::fmt::Debug;
use dashmap::DashMap;
use futures::{future::join_all, FutureExt};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::identity;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

pub struct EngineBuildConfig {
    pub supported_exchange_clients: HashMap<ExchangeId, Box<dyn ExchangeClientBuilder + 'static>>,
}

impl EngineBuildConfig {
    pub fn standard() -> Self {
        let exchange_name = "Binance".into();
        let supported_exchange_clients =
            hashmap![exchange_name => Box::new(BinanceBuilder) as Box<dyn ExchangeClientBuilder>];

        EngineBuildConfig {
            supported_exchange_clients,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum InitSettings<TStrategySettings>
where
    TStrategySettings: BaseStrategySettings + Clone,
{
    Directly(AppSettings<TStrategySettings>),
    Load(String, String),
}

pub async fn launch_trading_engine<'a, TStrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<TStrategySettings>,
    build_strategy: impl Fn(&AppSettings<TStrategySettings>) -> Box<dyn DispositionStrategy + 'static>,
) -> Result<TradingEngine>
where
    TStrategySettings: BaseStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    init_logger();

    info!("*****************************");
    info!("TradingEngine starting");

    let settings = match init_user_settings {
        InitSettings::Directly(v) => v,
        InitSettings::Load(config_path, credentials_path) => {
            load_settings::<TStrategySettings>(&config_path, &credentials_path)?
        }
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
        .map(|exchange| (exchange.exchange_account_id.clone(), exchange))
        .collect();

    let exchange_events = ExchangeEvents::new(events_sender);

    let (finish_graceful_shutdown_tx, finish_graceful_shutdown_rx) = oneshot::channel();
    let engine_context = EngineContext::new(
        settings.core.clone(),
        exchanges_map.clone(),
        exchange_events,
        finish_graceful_shutdown_tx,
        timeout_manager,
        application_manager.clone(),
    );

    let internal_events_loop = InternalEventsLoop::new();
    let control_panel = ControlPanel::new(
        "127.0.0.1:8080",
        toml::Value::try_from(settings.clone())?.to_string(),
        application_manager,
    );

    {
        let local_exchanges_map = exchanges_map.into_iter().map(identity).collect();
        let action = internal_events_loop.clone().start(
            events_receiver,
            local_exchanges_map,
            engine_context.application_manager.stop_token(),
        );
        let _ = spawn_future("internal_events_loop start", true, action.boxed());
    }

    if let Err(error) = control_panel.clone().start() {
        error!("Unable to start rest api: {}", error);
    }

    let disposition_strategy = build_strategy(&settings);
    let disposition_executor_service = create_disposition_executor_service(
        &settings.strategy,
        &engine_context,
        disposition_strategy,
    );

    engine_context.shutdown_service.register_services(&[
        control_panel,
        internal_events_loop,
        disposition_executor_service,
    ]);

    info!("TradingEngine started");
    Ok(TradingEngine::new(
        engine_context,
        finish_graceful_shutdown_rx,
    ))
}

fn create_disposition_executor_service(
    base_settings: &dyn BaseStrategySettings,
    engine_context: &Arc<EngineContext>,
    disposition_strategy: Box<dyn DispositionStrategy>,
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
    )
}

pub async fn create_exchanges(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
    events_channel: broadcast::Sender<ExchangeEvent>,
    application_manager: Arc<ApplicationManager>,
    timeout_manager: &Arc<TimeoutManager>,
) -> Vec<Arc<Exchange>> {
    let statistics = StatisticService::new();
    join_all(core_settings.exchanges.iter().map(|x| {
        create_exchange(
            x,
            build_settings,
            events_channel.clone(),
            application_manager.clone(),
            timeout_manager.clone(),
            statistics.clone(),
        )
    }))
    .await
}
