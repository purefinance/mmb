use super::common::{
    CurrencyPair, ExchangeAccountId, ExchangeErrorType, RestErrorDescription, RestRequestOutcome,
    SpecificCurrencyPair,
};
use crate::core::orders::fill::EventSourceType;
use crate::core::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo,
};
use async_trait::async_trait;
use log::info;

// Implementation of rest API client
#[async_trait(?Send)]
pub trait ExchangeClient: Support {
    async fn create_order(&self, _order: &OrderCreating) -> RestRequestOutcome;

    async fn cancel_order(&self, _order: &OrderCancelling) -> RestRequestOutcome;

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair);

    async fn get_account_info(&self);

    async fn request_open_orders(&self) -> RestRequestOutcome;
}

#[async_trait(?Send)]
pub trait Support {
    fn is_rest_error_code(&self, response: &RestRequestOutcome) -> Option<RestErrorDescription>;
    fn get_order_id(&self, response: &RestRequestOutcome) -> ExchangeOrderId;
    fn get_error_type(&self, error: &RestErrorDescription) -> ExchangeErrorType;

    fn on_websocket_message(&self, msg: &str);

    fn set_order_created_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>,
    );

    fn set_order_cancelled_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>,
    );

    fn build_ws_main_path(
        &self,
        specific_currency_pairs: &[SpecificCurrencyPair],
        websocket_channels: &[String],
    ) -> String;
    async fn build_ws_secondary_path(&self) -> String;

    // TODO has to be rewritten. Probably after getting metadata feature
    fn get_specific_currency_pair(&self, currency_pair: &CurrencyPair) -> SpecificCurrencyPair;

    fn should_log_message(&self, message: &str) -> bool;

    fn log_unknown_message(&self, exchange_account_id: ExchangeAccountId, message: &str) {
        info!("Unknown message for {}: {}", exchange_account_id, message);
    }

    fn parse_open_orders(&self, response: &RestRequestOutcome) -> Vec<OrderInfo>;
}
