use std::sync::{Arc, Weak};

use crate::exchanges::exchange_blocker::ExchangeBlocker;
use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use crate::lifecycle::launcher::EngineBuildConfig;
use crate::settings::ExchangeSettings;
use crate::{
    exchanges::{
        general::exchange::Exchange,
        timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory,
        timeouts::timeout_manager::TimeoutManager,
    },
    settings::CoreSettings,
};
use domain::events::ExchangeEvent;
use domain::exchanges::commission::Commission;
use domain::order::pool::OrdersPool;
use mmb_utils::infrastructure::WithExpect;
use tokio::sync::broadcast;

pub fn create_timeout_manager(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
) -> Arc<TimeoutManager> {
    let request_timeout_managers = core_settings
        .exchanges
        .iter()
        .map(|exchange_settings| {
            let timeout_arguments = build_settings.supported_exchange_clients
                [&exchange_settings.exchange_account_id.exchange_id]
                .get_timeout_arguments();

            let exchange_account_id = exchange_settings.exchange_account_id;
            let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
                timeout_arguments,
                exchange_account_id,
            );

            (exchange_account_id, request_timeout_manager)
        })
        .collect();

    TimeoutManager::new(request_timeout_managers)
}

pub async fn create_exchange(
    user_settings: &ExchangeSettings,
    build_settings: &EngineBuildConfig,
    events_channel: broadcast::Sender<ExchangeEvent>,
    lifetime_manager: Arc<AppLifetimeManager>,
    timeout_manager: Arc<TimeoutManager>,
    exchange_blocker: Weak<ExchangeBlocker>,
) -> Arc<Exchange> {
    let exchange_account_id = user_settings.exchange_account_id;
    let exchange_client_builder =
        &build_settings.supported_exchange_clients[&exchange_account_id.exchange_id];
    let orders = OrdersPool::new();

    let exchange_client = exchange_client_builder.create_exchange_client(
        user_settings.clone(),
        events_channel.clone(),
        lifetime_manager.clone(),
        timeout_manager.clone(),
        orders.clone(),
    );

    let exchange = Exchange::new(
        exchange_account_id,
        exchange_client.client,
        orders,
        exchange_client.features,
        exchange_client_builder.get_timeout_arguments(),
        events_channel,
        lifetime_manager,
        timeout_manager,
        exchange_blocker,
        Commission::default(),
    );

    exchange.build_symbols(&user_settings.currency_pairs).await;

    exchange
        .connect_ws()
        .await
        .with_expect(move || "Failed to connect to websockets on exchange {exchange_account_id}");

    exchange.exchange_client.initialized(exchange.clone()).await;

    exchange
}
