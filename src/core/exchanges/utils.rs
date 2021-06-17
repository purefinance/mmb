use anyhow::Result;
use futures::Future;
use log::{error, info, trace};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::{
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{application_manager::ApplicationManager, common::OPERATION_CANCELED_MSG};

pub(crate) fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

static APPLICATION_MANAGER: OnceCell<Mutex<Option<Arc<ApplicationManager>>>> = OnceCell::new();

pub(crate) fn keep_application_manager(application_manager: Arc<ApplicationManager>) {
    APPLICATION_MANAGER.get_or_init(|| Mutex::new(Some(application_manager)));
}

pub(crate) fn unset_application_manager() {
    APPLICATION_MANAGER.get().unwrap().lock().take();
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FutureOutcome {
    CompletedSuccessfully,
    Canceled,
    Error,
    Panicked,
}

pub type CustomSpawnFuture = Box<dyn Future<Output = Result<()>> + Send>;

// FIXME DOCUMENTATE IT
// FIXME BETTER NAME MAYBE
pub fn custom_spawn(
    action_name: &str,
    _timeout: Option<Duration>,
    action: Pin<CustomSpawnFuture>,
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

                    let panic = error.into_panic();
                    let maybe_error_msg = panic.as_ref().downcast_ref::<String>().clone();
                    if let Some(error_msg) = maybe_error_msg {
                        if error_msg.to_string() == OPERATION_CANCELED_MSG {
                            trace!("{} was cancelled via panic", log_template);

                            if !is_critical {
                                return FutureOutcome::Canceled;
                            }
                        }

                        error!("{}", error_message);
                        // FIXME Evgeniy, look at that blocking version I chose.
                        // That's because Mutex locking around await
                        spawn_graceful_shutdown(&log_template, &error_message);

                        return FutureOutcome::Panicked;
                    }
                }
                FutureOutcome::Canceled
            }
        }
    });

    handler
}

fn spawn_graceful_shutdown(log_template: &str, error_message: &str) {
    match APPLICATION_MANAGER.get() {
        Some(application_manager) => {
            let test = application_manager.lock();
            let manager = &*test;
            match manager {
                Some(application_manager) => {
                    application_manager.clone().spawn_graceful_shutdown(error_message.to_owned());
                }
                None => error!("Unable to start graceful shutdown after panic inside {} because there are no application manager",
                    log_template),
            }
        }
        None => error!("Unable to start graceful shutdown after panic inside {} because there are no application manager",
            log_template),
    }
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
        let future_outcome = custom_spawn("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::CompletedSuccessfully);

        Ok(())
    }

    #[tokio::test]
    async fn future_canceled_via_result() -> Result<()> {
        // Arrange
        let action = async { bail!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = custom_spawn("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn future_error() -> Result<()> {
        // Arrange
        let action = async { bail!("Some error") };

        // Act
        let future_outcome = custom_spawn("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Error);

        Ok(())
    }

    #[tokio::test]
    async fn non_critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome =
            custom_spawn("test_action_name", None, Box::pin(action), false).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = custom_spawn("test_action_name", None, Box::pin(action), true).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Panicked);

        Ok(())
    }
}
