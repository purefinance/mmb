use super::common::{
    CurrencyPair, ExchangeErrorType, RestErrorDescription, RestRequestOutcome, SpecificCurrencyPair,
};
use crate::core::orders::order::{ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo};
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait CommonInteraction {
    async fn create_order(&self, _order: &OrderCreating) -> RestRequestOutcome;

    fn get_specific_currency_pair(&self, currency_pair: &CurrencyPair) -> SpecificCurrencyPair;

    fn is_rest_error_code(&self, response: &RestRequestOutcome) -> Option<RestErrorDescription>;
    fn get_order_id(&self, response: &RestRequestOutcome) -> ExchangeOrderId;
    fn get_error_type(&self, error: &RestErrorDescription) -> ExchangeErrorType;

    async fn get_account_info(&self);

    async fn get_open_orders(&self);
    fn parse_get_open_orders(&self) -> OrderInfo;

    async fn cancel_order(&self, _order: &OrderCancelling) -> RestRequestOutcome;

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair);
}
