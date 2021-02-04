use super::common::{CurrencyPair, RestRequestOutcome};
use crate::core::orders::order::{DataToCancelOrder, DataToCreateOrder};
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait CommonInteraction {
    async fn create_order(&self, _order: &DataToCreateOrder) -> RestRequestOutcome {
        unimplemented!("It's generic trait and has no implementations");
    }

    async fn get_account_info(&self) {
        unimplemented!("It's generic trait and has no implementations");
    }

    async fn cancel_order(&self, _order: &DataToCancelOrder) -> RestRequestOutcome {
        unimplemented!("It's generic trait and has no implementations");
    }

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair) {
        unimplemented!("It's generic trait and has no implementations");
    }
}
