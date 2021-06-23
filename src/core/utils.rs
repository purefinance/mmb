use anyhow::{bail, Result};
use futures::Future;
use futures::FutureExt;
use log::{error, info, trace, Level};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::panic;
use std::{
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::exchanges::{application_manager::ApplicationManager, common::OPERATION_CANCELED_MSG};

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
    match APPLICATION_MANAGER.get() {
        Some(application_manager) => application_manager.lock().take(),
        None => panic!("Attempt to unset static application manager before it has been set"),
    };
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FutureOutcome {
    CompletedSuccessfully,
    Canceled,
    Error,
    Panicked,
    TimeExpired,
}

impl From<FutureOutcome> for Result<()> {
    fn from(future_outcome: FutureOutcome) -> Self {
        match future_outcome {
            FutureOutcome::Error => bail!("Future returned error"),
            FutureOutcome::Panicked => bail!("Future panicked"),
            FutureOutcome::TimeExpired => bail!("Time is up for future execution"),
            FutureOutcome::Canceled => bail!("Future canceled"),
            FutureOutcome::CompletedSuccessfully => Ok(()),
        }
    }
}

pub type CustomSpawnFuture = Box<dyn Future<Output = Result<()>> + Send>;

/// Spawn future with timer. Error will be logged if times up before action completed
/// Other nuances are the same as custom_spawn()
pub fn custom_spawn_timed(
    action_name: &str,
    is_critical: bool,
    duration: Duration,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let action = custom_spawn(&action_name, is_critical, action);

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                error!("Time in form of {:?} is over, but future {} is not completed yet", duration, action_name);
                return FutureOutcome::TimeExpired;
            }
            action_outcome = action => {
                match action_outcome {
                    Ok(outcome) => return outcome,
                    Err(_) => {
                        // JoinHandle fron action_outcome are not aborting anywhere
                        // So only available option here is panic somewhere spawn_future()
                        error!("Custom_spawn() panicked");
                        FutureOutcome::Panicked
                    }
                }
            }
        }
    })
}

/// Spawn future with logging and error, panic and cancellatioin handling
/// Inside the crate prefer this function to all others
pub fn custom_spawn(
    action_name: &str,
    is_critical: bool,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();
    let log_template = format!("Future {}, with id {}", action_name, future_id);

    info!("{} started", log_template);

    tokio::spawn(async move {
        let action_outcome = panic::AssertUnwindSafe(action).catch_unwind().await;

        match action_outcome {
            Ok(future_outcome) => match future_outcome {
                Ok(()) => {
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
            Err(panic) => match panic.as_ref().downcast_ref::<String>().clone() {
                Some(error_msg) => {
                    if error_msg.to_string() == OPERATION_CANCELED_MSG {
                        let log_level = if is_critical {
                            Level::Error
                        } else {
                            Level::Trace
                        };
                        log::log!(log_level, "{} was cancelled via panic", log_template);

                        if !is_critical {
                            return FutureOutcome::Canceled;
                        }
                    }

                    let error_message =
                        format!("{} panicked with error: {}", log_template, error_msg);
                    error!("{}", error_message);

                    spawn_graceful_shutdown(&log_template, &error_message);

                    return FutureOutcome::Panicked;
                }
                None => {
                    let error_message = format!("{} panicked with non string error", log_template);
                    error!("{}", error_message);

                    spawn_graceful_shutdown(&log_template, &error_message);

                    return FutureOutcome::Panicked;
                }
            },
        }
    })
}

fn spawn_graceful_shutdown(log_template: &str, error_message: &str) {
    match APPLICATION_MANAGER.get() {
        Some(application_manager) => {
            match &*application_manager.lock() {
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
    use crate::core::exchanges::cancellation_token::CancellationToken;

    use super::*;
    use anyhow::{bail, Result};
    use futures::FutureExt;

    #[tokio::test]
    async fn future_completed_successfully() -> Result<()> {
        // Arrange
        let action = async { Ok(()) };

        // Act
        let future_outcome = custom_spawn("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::CompletedSuccessfully);

        Ok(())
    }

    #[tokio::test]
    async fn future_canceled_via_result() -> Result<()> {
        // Arrange
        let action = async { bail!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = custom_spawn("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn future_error() -> Result<()> {
        // Arrange
        let action = async { bail!("Some error") };

        // Act
        let future_outcome = custom_spawn("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Error);

        Ok(())
    }

    #[tokio::test]
    async fn non_critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = custom_spawn("test_action_name", false, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = custom_spawn("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Panicked);

        Ok(())
    }

    #[tokio::test]
    async fn panic_with_application_manager() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        let application_manager = ApplicationManager::new(CancellationToken::new());
        keep_application_manager(application_manager);

        // Act
        let future_outcome = custom_spawn("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome, FutureOutcome::Panicked);

        Ok(())
    }

    #[tokio::test]
    async fn future_aborted() {
        // Arrange
        let test_value = Arc::new(Mutex::new(false));
        let test_to_future = test_value.clone();
        let action = async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            (*test_to_future.lock()) = true;

            Ok(())
        };

        // Act
        let future_outcome = custom_spawn("test_action_name", true, action.boxed());
        future_outcome.abort();

        // Assert
        assert_eq!(*test_value.lock(), false);
    }

    mod with_timer {
        use super::*;

        #[tokio::test]
        async fn time_is_over() -> Result<()> {
            // Arrange
            let action = async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(())
            };

            // Act
            let future_outcome = custom_spawn_timed(
                "test_action_name",
                true,
                Duration::from_secs(0),
                action.boxed(),
            )
            .await?;

            // Assert
            assert_eq!(future_outcome, FutureOutcome::TimeExpired);

            Ok(())
        }

        #[tokio::test]
        async fn error_in_action() -> Result<()> {
            // Arrange
            let action = async { bail!("Some error for test") };

            // Act
            let future_outcome = custom_spawn_timed(
                "test_action_name",
                true,
                Duration::from_millis(200),
                action.boxed(),
            )
            .await?;

            // Assert
            assert_eq!(future_outcome, FutureOutcome::Error);

            Ok(())
        }

        #[tokio::test]
        async fn action_completed_in_time() -> Result<()> {
            // Arrange
            let action = async { Ok(()) };

            // Act
            let future_outcome = custom_spawn_timed(
                "test_action_name",
                true,
                Duration::from_millis(200),
                action.boxed(),
            )
            .await?;

            // Assert
            assert_eq!(future_outcome, FutureOutcome::CompletedSuccessfully);

            Ok(())
        }

        #[tokio::test]
        async fn timed_future_aborted() {
            // Arrange
            let test_value = Arc::new(Mutex::new(false));
            let test_to_future = test_value.clone();
            let action = async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                (*test_to_future.lock()) = true;

                Ok(())
            };

            // Act
            let future_outcome = custom_spawn_timed(
                "test_action_name",
                true,
                Duration::from_millis(200),
                action.boxed(),
            );
            future_outcome.abort();

            // Assert
            assert_eq!(*test_value.lock(), false);
        }
    }
}
