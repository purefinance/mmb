use super::common::{CurrencyPair, RestRequestOutcome};
use crate::core::orders::order::{OrderCancelling, OrderCreating};
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait CommonInteraction {
    async fn create_order(&self, _order: &OrderCreating) -> RestRequestOutcome;

    async fn get_account_info(&self);

    async fn cancel_order(&self, _order: &OrderCancelling) -> RestRequestOutcome;

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair);
}
