use crate::core::exchanges::common::{ExchangeAccountId, ExchangeId};
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
use crate::core::{config::load_settings, statistic_service::StatisticEventHandler};
use crate::core::{
    disposition_execution::executor::DispositionExecutorService,
    infrastructure::{keep_application_manager, spawn_future},
};
use crate::core::{
    exchanges::binance::binance::BinanceBuilder, statistic_service::StatisticService,
};
use crate::hashmap;
use crate::rest_api::control_panel::ControlPanel;
use crate::strategies::disposition_strategy::DispositionStrategy;
use anyhow::{bail, Result};
use core::fmt::Debug;
use dashmap::DashMap;
use futures::{future::join_all, FutureExt};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::identity;
use std::panic::{self, AssertUnwindSafe};
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

async fn before_enging_context_init<'a, TStrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<TStrategySettings>,
) -> Result<(
    broadcast::Sender<ExchangeEvent>,
    broadcast::Receiver<ExchangeEvent>,
    AppSettings<TStrategySettings>,
    Arc<ApplicationManager>,
    DashMap<ExchangeAccountId, Arc<Exchange>>,
    Arc<EngineContext>,
    oneshot::Receiver<()>,
)>
where
    TStrategySettings: BaseStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    init_logger();
    //panic!("WOW");

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

    let exchange_events = ExchangeEvents::new(events_sender.clone());

    let (finish_graceful_shutdown_tx, finish_graceful_shutdown_rx) = oneshot::channel();
    let engine_context = EngineContext::new(
        settings.core.clone(),
        exchanges_map.clone(),
        exchange_events,
        finish_graceful_shutdown_tx,
        timeout_manager,
        application_manager.clone(),
    );

    Ok((
        events_sender,
        events_receiver,
        settings,
        application_manager,
        exchanges_map,
        engine_context,
        finish_graceful_shutdown_rx,
    ))
}

pub async fn launch_trading_engine<'a, TStrategySettings>(
    build_settings: &EngineBuildConfig,
    init_user_settings: InitSettings<TStrategySettings>,
    build_strategy: impl Fn(
        &AppSettings<TStrategySettings>,
        Arc<EngineContext>,
    ) -> Box<dyn DispositionStrategy + 'static>,
) -> Result<TradingEngine>
where
    TStrategySettings: BaseStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    let action_outcome = AssertUnwindSafe(before_enging_context_init(
        build_settings,
        init_user_settings,
    ))
    .catch_unwind()
    .await;

    let (
        events_sender,
        events_receiver,
        settings,
        application_manager,
        exchanges_map,
        engine_context,
        finish_graceful_shutdown_rx,
    ) = match action_outcome {
        Ok(outcome) => outcome,
        Err(panic) => {
            match panic.as_ref().downcast_ref::<&str>().clone() {
                Some(panic_message) => {
                    error!(
                        "Panic happend during EngineContext creation: {}",
                        panic_message
                    );
                }
                None => {
                    error!("Panic happend during EngineContext creation without readable message")
                }
            }
            bail!("Panic during EnginContext creation")
        }
    }?;

    let _cloned_engine_context = engine_context.clone();

    run_services();
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        panic!("test");

        let internal_events_loop = InternalEventsLoop::new();
        engine_context
            .shutdown_service
            .register_service(internal_events_loop.clone());

        let exchange_events = ExchangeEvents::new(events_sender.clone());
        let statistic_service = StatisticService::new();
        let statistic_event_handler =
            create_statistic_event_handler(exchange_events, statistic_service.clone());
        let control_panel = ControlPanel::new(
            "127.0.0.1:8080",
            toml::Value::try_from(settings.clone())?.to_string(),
            application_manager.clone(),
            statistic_service.clone(),
        );
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

        if let Err(error) = control_panel.clone().start() {
            error!("Unable to start rest api: {}", error);
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

        //engine_context.shutdown_service.register_services(&[
        //    control_panel,
        //    internal_events_loop,
        //    disposition_executor_service,
        //]);

        info!("TradingEngine started");
        Ok(TradingEngine::new(
            engine_context,
            finish_graceful_shutdown_rx,
        ))
    }));

    match result {
        Ok(trading_engine) => trading_engine,
        Err(panic) => {
            match panic.as_ref().downcast_ref::<&str>().clone() {
                Some(panic_message) => {
                    error!(
                        "Panic happend during TradingEngine creation: {}",
                        panic_message
                    );
                }
                None => {
                    error!("Panic happend during TradingEngine creation without readable message")
                }
            }

            application_manager
                .run_graceful_shutdown("Panic during TradeingEngine creation")
                .await;
            bail!("Panic during EnginContext creation")
        }
    }
}

fn run_services() -> () {
    todo!()
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
