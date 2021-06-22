use anyhow::{bail, Result};
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
    APPLICATION_MANAGER.get().unwrap().lock().take();
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
pub fn custom_spawn_timered(
    action_name: &'static str,
    is_critical: bool,
    duration: Duration,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    let action = custom_spawn(action_name, is_critical, action);
    let timer = async move {
        tokio::time::sleep(duration).await;
    };

    tokio::spawn(async move {
        tokio::select! {
            _ = timer => {
                error!("Time is over, but future {} is not completed yet", action_name);
                return FutureOutcome::TimeExpired;
            }
            action_outcome = action => {
                match action_outcome {
                    Ok(outcome) => return outcome,
                    Err(_) => {
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

    let action_outcome = tokio::spawn(action);

    // FIXME explore how tokio handle can catch panics
    tokio::spawn(async move {
        // FIXME catch_unwind and simple graceful_shutdown
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
                        spawn_graceful_shutdown(&log_template, &error_message);

                        return FutureOutcome::Panicked;
                    }
                }
                FutureOutcome::Canceled
            }
        }
    })
}

fn spawn_graceful_shutdown(log_template: &str, error_message: &str) {
    match APPLICATION_MANAGER.get() {
        Some(application_manager) => {
            //let test = application_manager.lock();
            //let manager = &*test;
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
            let future_outcome = custom_spawn_timered(
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
            let future_outcome = custom_spawn_timered(
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
            let future_outcome = custom_spawn_timered(
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
    }
}
