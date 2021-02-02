use super::common::CurrencyPair;
use crate::core::orders::order::DataToCreateOrder;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait CommonInteraction {
    async fn create_order(&self, order: &DataToCreateOrder) {
        unimplemented!("It's generic trait and has no implementations");
    }
    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) {
        unimplemented!("It's generic trait and has no implementations");
    }
}
