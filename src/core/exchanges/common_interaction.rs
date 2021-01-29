use async_trait::async_trait;

// TODO dangerous!!! Not thread-safe, read more about it
#[async_trait(?Send)]
pub trait CommonInteraction {
    async fn create_order(&self) {}
    async fn cancel_order(&self) {}
}
