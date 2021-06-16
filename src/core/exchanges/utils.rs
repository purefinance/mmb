use log::{error, info, trace};
use std::{
    pin::Pin,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{
    application_manager::ApplicationManager, common::OPERATION_CANCELED_MSG,
    timeouts::timeout_manager::BoxFuture,
};

pub(crate) fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FutureOutcome {
    CompletedSuccessfully,
    Canceled,
    Error,
    Panicked,
}

pub(crate) fn spawn_task(
    action_name: &str,
    _timeout: Option<Duration>,
    action: Pin<BoxFuture>,
    is_critical: bool,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();
    let log_template = format!("Future {}, with id {}", action_name, future_id);

    info!("{} started", log_template);

    let action_outcome = tokio::spawn(async move { action.await });

    let handler = tokio::spawn(async move {
        match action_outcome.await {
            Ok(future_outcome) => match future_outcome {
                Ok(_) => {
                    trace!("{} successfully completed", log_template);
                    return FutureOutcome::CompletedSuccessfully;
                }
                Err(error) => {
                    if error.to_string() == OPERATION_CANCELED_MSG {
                        trace!("{} was cancelled via Result<()>", log_template);

                        return FutureOutcome::Canceled;
                    }

                    error!("{} returned error: {:?}", log_template, error);
                    return FutureOutcome::Error;
                }
            },
            Err(error) => {
                if error.is_panic() {
                    let error_message =
                        format!("{} panicked with error: {:?}", log_template, error);

                    if let Some(error_msg) = error.into_panic().as_ref().downcast_ref::<String>() {
                        if error_msg.to_string() == OPERATION_CANCELED_MSG {
                            trace!("{} was cancelled via panic", log_template);

                            if !is_critical {
                                return FutureOutcome::Canceled;
                            }
                        }

                        error!("{}", error_message);
                        //application_manager
                        //    .run_graceful_shutdown(&error_message)
                        //    .await;

                        return FutureOutcome::Panicked;
                    }
                }
                FutureOutcome::Canceled
            }
        }
    });

    handler
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{bail, Result};

    #[tokio::test]
    async fn future_completed_successfully() -> Result<()> {
        // Arrange
        let action = async { Ok(()) };

        // Act
        let future_outcome = spawn_task("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::CompletedSuccessfully);

        Ok(())
    }

    #[tokio::test]
    async fn future_canceled_via_result() -> Result<()> {
        // Arrange
        let action = async { bail!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_task("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn future_error() -> Result<()> {
        // Arrange
        let action = async { bail!("Some error") };

        // Act
        let future_outcome = spawn_task("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Error);

        Ok(())
    }

    #[tokio::test]
    async fn non_critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_task("test_action_name", None, Box::pin(action), false).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_task("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Panicked);

        Ok(())
    }
}
