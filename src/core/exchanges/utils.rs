use anyhow::Result;
use futures::Future;
use log::info;
use std::{
    pin::Pin,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

pub type BoxFuture = Box<dyn Future<Output = Result<()>> + Sync + Send>;

pub(crate) fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

pub(crate) fn spawn_task(
    action_name: &str,
    service_name: &str,
    _timeout: Option<Duration>,
    action: Pin<BoxFuture>,
) -> JoinHandle<()> {
    let future_id = Uuid::new_v4();
    info!("Future {} with id {} started", action_name, future_id);

    let handler = tokio::spawn(async move {
        let future_outcome = action.await;
        match future_outcome {
            Ok(_) => {}
            Err(_) => {}
        }
    });
    handler
}

#[cfg(test)]
mod test {

    use futures::future::ready;

    use super::*;

    #[tokio::test]
    async fn first_test() {
        // Arrange
        // Act
        spawn_task(
            "test_action_name",
            "test_service_name",
            None,
            Box::new(ready(Ok(()))),
        );
        // Assert
    }
}
