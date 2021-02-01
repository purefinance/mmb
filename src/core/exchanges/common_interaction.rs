use crate::core::orders::order::OrderSnapshot;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait CommonInteraction {
    async fn create_order(&self, order: &OrderSnapshot) {
        unimplemented!("It's generic trait and has no implementations");
    }
    async fn cancel_order(&self) {
        unimplemented!("It's generic trait and has no implementations");
    }
}
