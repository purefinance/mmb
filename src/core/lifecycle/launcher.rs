use crate::control_api::health_check::start_control_server;
use crate::core::exchanges::binance::binance::BinanceBuilder;
use crate::core::exchanges::common::ExchangeId;
use crate::core::exchanges::events::{ExchangeEvents, CHANNEL_MAX_EVENTS_COUNT};
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::general::exchange_creation::create_exchange;
use crate::core::exchanges::traits::ExchangeClientBuilder;
use crate::core::logger::init_logger;
use crate::core::settings::{AppSettings, CoreSettings};
use crate::hashmap;
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

pub async fn launch_trading_engine<TSettings: Default>(build_settings: &EngineBuildConfig) {
    init_logger();

    info!("*****************************");
    info!("Bot started session");

    let settings = load_settings::<TSettings>().await;
    let exchanges = create_exchanges(&settings.core, build_settings).await;
    let _exchanges_map: HashMap<_, _> = exchanges
        .into_iter()
        .map(|x| (x.exchange_account_id.clone(), x))
        .collect();

    let (events_sender, _events_receiver) = broadcast::channel(CHANNEL_MAX_EVENTS_COUNT);

    let _exchange_events = ExchangeEvents::new(events_sender);

    {
        // TODO uncomment when will be implemented Send for Exchange;
        // let exchanges_map = exchanges_map.clone();
        // let _ = tokio::spawn(
        //     async move { ExchangeEvents::start(events_receiver, exchanges_map).await },
        // );
    }

    // TODO how to handle result here? Probably graceful shutdown?
    let _ = start_control_server("127.0.0.1:8080").await;
}

async fn load_settings<TSettings: Default>() -> AppSettings<TSettings> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_rt::test]
    async fn launch_engine() {
        let config = EngineBuildConfig::standard();
        launch_trading_engine::<()>(&config).await;
    }
}
