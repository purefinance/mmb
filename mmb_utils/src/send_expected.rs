use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};

static UNABLE_TO_SEND: &str = "Unable to send event";

pub trait SendExpected<T>
where
    T: Send,
{
    fn send_expected(self, value: T);
}

impl<T> SendExpected<T> for oneshot::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    fn send_expected(self, value: T) {
        self.send(value).expect(UNABLE_TO_SEND);
    }
}

#[async_trait]
pub trait SendExpectedAsync<T>
where
    T: Send + std::fmt::Debug,
{
    async fn send_expected_async(&self, value: T);
}

#[async_trait]
impl<T> SendExpectedAsync<T> for mpsc::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    async fn send_expected_async(&self, value: T) {
        self.send(value).await.expect(UNABLE_TO_SEND);
    }
}

pub trait SendExpectedByRef<T>
where
    T: Send + std::fmt::Debug,
{
    fn send_expected(&self, value: T);
}

impl<T> SendExpectedByRef<T> for broadcast::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    fn send_expected(&self, value: T) {
        self.send(value).expect(UNABLE_TO_SEND);
    }
}

impl<T> SendExpectedByRef<T> for mpsc::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    fn send_expected(&self, value: T) {
        self.try_send(value).expect(UNABLE_TO_SEND);
    }
}

impl<T> SendExpectedByRef<T> for std::sync::mpsc::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    fn send_expected(&self, value: T) {
        self.send(value).expect(UNABLE_TO_SEND);
    }
}
