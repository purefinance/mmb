use crate::interactive_brokers::InteractiveBrokers;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    RestFillsType, WebSocketOptions,
};
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::exchanges::traits::{ExchangeClientBuilder, ExchangeClientBuilderResult};
use mmb_core::lifecycle::app_lifetime_manager::AppLifetimeManager;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_domain::market::ExchangeId;
use mmb_domain::order::pool::OrdersPool;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub struct InteractiveBrokersBuilder;

impl ExchangeClientBuilder for InteractiveBrokersBuilder {
    fn create_exchange_client(
        &self,
        _exchange_settings: ExchangeSettings,
        _events_channel: Sender<ExchangeEvent>,
        _lifetime_manager: Arc<AppLifetimeManager>,
        _timeout_manager: Arc<TimeoutManager>,
        _orders: Arc<OrdersPool>,
    ) -> ExchangeClientBuilderResult {
        let empty_response_is_ok = false;

        ExchangeClientBuilderResult {
            client: Box::new(InteractiveBrokers::new()),
            features: ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::new(RestFillsType::None),
                OrderFeatures {
                    supports_get_order_info_by_client_order_id: true,
                    ..OrderFeatures::default()
                },
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                empty_response_is_ok,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        }
    }

    /// TODO: Check if it is right
    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        // TODO: Check if it is right
        RequestTimeoutArguments::from_requests_per_minute(1200)
    }

    fn get_exchange_id(&self) -> ExchangeId {
        "IBKR".into()
    }
}
