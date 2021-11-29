use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::core::infrastructure::WithExpect;

static UNABLE_TO_SEND: &'static str = "Unable to send event";

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
        let _ = self
            .send(value)
            .with_expect(|| format!("{}", UNABLE_TO_SEND));
    }
}

pub trait TrySendExpected<T>
where
    T: Send + std::fmt::Debug,
{
    fn try_send_expected(&self, value: T);
}

impl<T> TrySendExpected<T> for mpsc::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    fn try_send_expected(&self, value: T) {
        let _ = self
            .try_send(value)
            .with_expect(|| format!("{}", UNABLE_TO_SEND));
    }
}

#[async_trait]
pub trait SendExpectedAsync<T>
where
    T: Send + std::fmt::Debug,
{
    async fn send_expected(&self, value: T);
}

#[async_trait]
impl<T> SendExpectedAsync<T> for mpsc::Sender<T>
where
    T: Send + std::fmt::Debug,
{
    async fn send_expected(&self, value: T) {
        let _ = self
            .send(value)
            .await
            .with_expect(|| format!("{}", UNABLE_TO_SEND));
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
        let _ = self
            .send(value)
            .with_expect(|| format!("{}", UNABLE_TO_SEND));
    }
}
