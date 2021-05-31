use crate::core::exchanges::common::ExchangeId;
use crate::core::exchanges::events::{ExchangeEvents, CHANNEL_MAX_EVENTS_COUNT};
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::general::exchange_creation::create_exchange;
use crate::core::exchanges::traits::ExchangeClientBuilder;
use crate::core::internal_events_loop::InternalEventsLoop;
use crate::core::lifecycle::trading_engine::EngineContext;
use crate::core::logger::init_logger;
use crate::core::settings::{AppSettings, CoreSettings};
use crate::hashmap;
use crate::{
    core::exchanges::binance::binance::BinanceBuilder, rest_api::control_panel::ControlPanel,
};
use futures::future::join_all;
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct EngineBuildConfig {
    pub supported_exchange_clients: HashMap<ExchangeId, Box<dyn ExchangeClientBuilder + 'static>>,
}

impl EngineBuildConfig {
    pub fn standard() -> Self {
        let exchange_name = "binance".into();
        let supported_exchange_clients =
            hashmap![exchange_name => Box::new(BinanceBuilder) as Box<dyn ExchangeClientBuilder>];

        EngineBuildConfig {
            supported_exchange_clients,
        }
    }
}

pub async fn launch_trading_engine<TSettings: Default + Clone>(
    build_settings: &EngineBuildConfig,
) -> Arc<EngineContext> {
    init_logger();

    info!("*****************************");
    info!("Bot started session");

    let settings = load_settings::<TSettings>().await;
    let exchanges = create_exchanges(&settings.core, build_settings).await;
    let exchanges_map: HashMap<_, _> = exchanges
        .into_iter()
        .map(|x| (x.exchange_account_id.clone(), x))
        .collect();

    let (events_sender, events_receiver) = broadcast::channel(CHANNEL_MAX_EVENTS_COUNT);
    let exchange_events = ExchangeEvents::new(events_sender);

    let engine_context = EngineContext::new(settings.core.clone(), exchange_events);

    {
        let exchanges_map = exchanges_map.clone();
        let internal_events_loop = InternalEventsLoop::new();
        engine_context
            .shutdown_service
            .register_service(internal_events_loop.clone());
        let _ = tokio::spawn(internal_events_loop.start(
            events_receiver,
            exchanges_map,
            engine_context.application_manager.stop_token(),
        ));
    }

    ControlPanel::new("127.0.0.1:8080").start();

    engine_context
}

async fn load_settings<TSettings: Default + Clone>() -> AppSettings<TSettings> {
    // TODO implement load settings
    AppSettings::default()
}

pub async fn create_exchanges(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
) -> Vec<Arc<Exchange>> {
    join_all(
        core_settings
            .exchanges
            .iter()
            .map(|x| create_exchange(x, build_settings)),
    )
    .await
}
