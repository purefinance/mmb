use super::{
    common::{
        CurrencyPair, ExchangeAccountId, ExchangeError, RestRequestOutcome, SpecificCurrencyPair,
    },
    general::currency_pair_metadata::CurrencyPairMetadata,
};
// use crate::core::exchanges::common::Symbol;
use crate::core::exchanges::general::exchange::BoxExchangeClient;
use crate::core::exchanges::general::features::ExchangeFeatures;
use crate::core::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo,
};
use crate::core::orders::{fill::EventSourceType, order::OrderSnapshot};
use crate::core::settings::ExchangeSettings;
use anyhow::Result;
use async_trait::async_trait;
use log::info;
use std::sync::Arc;

// Implementation of rest API client
#[async_trait]
pub trait ExchangeClient: Support {
    // async fn create_order(&self, _order: &OrderCreating) -> Result<RestRequestOutcome>;
    //
    // async fn request_cancel_order(&self, _order: &OrderCancelling) -> Result<RestRequestOutcome>;
    //
    // async fn cancel_all_orders(&self, _currency_pair: CurrencyPair) -> Result<()>;
    //
    // async fn request_open_orders(&self) -> Result<RestRequestOutcome>;
    //
    // async fn request_order_info(&self, order: &OrderSnapshot) -> Result<RestRequestOutcome>;

    async fn request_metadata(&self) -> Result<RestRequestOutcome>;
}

#[async_trait]
pub trait Support {
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

    fn build_ws_main_path(
        &self,
        specific_currency_pairs: &[SpecificCurrencyPair],
        websocket_channels: &[String],
    ) -> String;
    async fn build_ws_secondary_path(&self) -> Result<String>;

    // TODO has to be rewritten. Probably after getting metadata feature
    fn get_specific_currency_pair(&self, currency_pair: &CurrencyPair) -> SpecificCurrencyPair;

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
}

pub trait ExchangeClientBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
    ) -> (BoxExchangeClient, ExchangeFeatures);
}
