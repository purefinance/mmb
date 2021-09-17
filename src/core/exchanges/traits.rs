use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use log::info;
use tokio::sync::broadcast;

use super::{
    common::CurrencyCode,
    common::CurrencyId,
    common::{
        CurrencyPair, ExchangeAccountId, ExchangeError, RestRequestOutcome, SpecificCurrencyPair,
    },
    general::currency_pair_metadata::{BeforeAfter, CurrencyPairMetadata},
    general::handlers::handle_order_filled::FillEventData,
    timeouts::requests_timeout_manager_factory::RequestTimeoutArguments,
};
use crate::core::exchanges::events::ExchangeEvent;
use crate::core::exchanges::general::features::ExchangeFeatures;
use crate::core::lifecycle::application_manager::ApplicationManager;
use crate::core::orders::fill::EventSourceType;
use crate::core::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo,
};
use crate::core::settings::ExchangeSettings;
use crate::core::{connectivity::connectivity_manager::WebSocketRole, orders::order::OrderSide};
use crate::core::{exchanges::general::exchange::BoxExchangeClient, orders::pool::OrderRef};
use awc::http::Uri;

// Implementation of rest API client
#[async_trait]
pub trait ExchangeClient: Support {
    async fn request_metadata(&self) -> Result<RestRequestOutcome>;

    async fn create_order(&self, order: &OrderCreating) -> Result<RestRequestOutcome>;

    async fn request_cancel_order(&self, order: &OrderCancelling) -> Result<RestRequestOutcome>;

    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()>;

    async fn request_open_orders(&self) -> Result<RestRequestOutcome>;

    async fn request_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<RestRequestOutcome>;

    async fn request_order_info(&self, order: &OrderRef) -> Result<RestRequestOutcome>;
}

#[async_trait]
pub trait Support: Send + Sync {
    fn is_rest_error_code(&self, response: &RestRequestOutcome) -> Result<(), ExchangeError>;
    fn get_order_id(&self, response: &RestRequestOutcome) -> Result<ExchangeOrderId>;
    fn clarify_error_type(&self, error: &mut ExchangeError);

    fn on_websocket_message(&self, msg: &str) -> Result<()>;

    fn set_order_created_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    );

    fn set_order_cancelled_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    );

    fn set_handle_order_filled_callback(
        &self,
        callback: Box<dyn FnMut(FillEventData) + Send + Sync>,
    );

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>);

    fn is_websocket_enabled(&self, role: WebSocketRole) -> bool;

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Uri>;

    fn get_specific_currency_pair(&self, currency_pair: &CurrencyPair) -> SpecificCurrencyPair;

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode>;

    fn should_log_message(&self, message: &str) -> bool;

    fn log_unknown_message(&self, exchange_account_id: ExchangeAccountId, message: &str) {
        info!("Unknown message for {}: {}", exchange_account_id, message);
    }

    fn parse_open_orders(&self, response: &RestRequestOutcome) -> Result<Vec<OrderInfo>>;
    fn parse_order_info(&self, response: &RestRequestOutcome) -> Result<OrderInfo>;
    fn parse_metadata(
        &self,
        response: &RestRequestOutcome,
    ) -> Result<Vec<Arc<CurrencyPairMetadata>>>;

    fn get_balance_reservation_currency_code(
        &self,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> CurrencyCode {
        currency_pair_metadata.get_trade_code(side, BeforeAfter::Before)
    }
}

pub struct ExchangeClientBuilderResult {
    pub client: BoxExchangeClient,
    pub features: ExchangeFeatures,
}

pub trait ExchangeClientBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        application_manager: Arc<ApplicationManager>,
    ) -> ExchangeClientBuilderResult;

    fn extend_settings(&self, settings: &mut ExchangeSettings);

    fn get_timeout_argments(&self) -> RequestTimeoutArguments;
}
