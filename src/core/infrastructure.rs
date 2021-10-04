use anyhow::{bail, Result};
use futures::Future;
use futures::FutureExt;
use log::{error, info, trace, Level};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::fmt::Display;
use std::panic;
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::lifecycle::application_manager::ApplicationManager;
use crate::core::OPERATION_CANCELED_MSG;

static APPLICATION_MANAGER: OnceCell<Mutex<Option<Arc<ApplicationManager>>>> = OnceCell::new();

pub(crate) fn keep_application_manager(application_manager: Arc<ApplicationManager>) {
    APPLICATION_MANAGER.get_or_init(|| Mutex::new(Some(application_manager)));
}

pub(crate) fn unset_application_manager() {
    match APPLICATION_MANAGER.get() {
        Some(application_manager) => application_manager.lock().take(),
        None => panic!(
            "Attempt to unset static application manager for spawn_future() before it has been set"
        ),
    };
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FutureOutcome {
    name: String,
    id: Uuid,
    completion_reason: CompletionReason,
}

impl FutureOutcome {
    pub fn new(name: String, id: Uuid, completion_reason: CompletionReason) -> Self {
        Self {
            name,
            id,
            completion_reason,
        }
    }

    pub fn into_result(&self) -> Result<()> {
        match self.completion_reason {
            CompletionReason::Error => {
                bail!("Future {} with id {} returned error", self.name, self.id)
            }
            CompletionReason::Panicked => {
                bail!("Future {} with id {} panicked", self.name, self.id)
            }
            CompletionReason::TimeExpired => bail!(
                "Time is up for future {} with id {} execution",
                self.name,
                self.id
            ),
            CompletionReason::Canceled => {
                bail!("Future {} with id {} canceled", self.name, self.id)
            }
            CompletionReason::CompletedSuccessfully => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CompletionReason {
    CompletedSuccessfully,
    Canceled,
    Error,
    Panicked,
    TimeExpired,
}

pub type CustomSpawnFuture = Box<dyn Future<Output = Result<()>> + Send>;

/// Spawn future with timer. Error will be logged if times up before action completed
/// Other nuances are the same as spawn_future()
pub fn spawn_future_timed(
    action_name: &str,
    is_critical: bool,
    duration: Duration,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();
    let action = handle_action_outcome(action_name.clone(), future_id, is_critical, action);

    info!("Future {} with id {} started", action_name, future_id);

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                error!("Time in form of {:?} is over, but future {} is not completed yet", duration, action_name);
                FutureOutcome::new(action_name, future_id, CompletionReason::TimeExpired)
            }
            action_outcome = action => {
                action_outcome
            }
        }
    })
}

/// Spawn future with logging and error, panic and cancellatioin handling
/// Inside the crate prefer this function to all others
pub fn spawn_future(
    action_name: &str,
    is_critical: bool,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();

    info!("Future {} with id {} started", action_name, future_id);

    tokio::spawn(handle_action_outcome(
        action_name,
        future_id,
        is_critical,
        action,
    ))
}

async fn handle_action_outcome(
    action_name: String,
    future_id: Uuid,
    is_critical: bool,
    action: Pin<CustomSpawnFuture>,
) -> FutureOutcome {
    let log_template = format!("Future {}, with id {}", action_name, future_id);
    let action_outcome = panic::AssertUnwindSafe(action).catch_unwind().await;

    match action_outcome {
        Ok(future_outcome) => match future_outcome {
            Ok(()) => {
                trace!("{} successfully completed", log_template);

                FutureOutcome::new(
                    action_name,
                    future_id,
                    CompletionReason::CompletedSuccessfully,
                )
            }
            Err(error) => {
                if error.to_string() == OPERATION_CANCELED_MSG {
                    trace!("{} was cancelled via Result<()>", log_template);

                    return FutureOutcome::new(action_name, future_id, CompletionReason::Canceled);
                }

                error!("{} returned error: {:?}", log_template, error);
                return FutureOutcome::new(action_name, future_id, CompletionReason::Error);
            }
        },
        Err(panic) => match panic.as_ref().downcast_ref::<String>().clone() {
            Some(error_msg) => {
                if error_msg == OPERATION_CANCELED_MSG {
                    let log_level = if is_critical {
                        Level::Error
                    } else {
                        Level::Trace
                    };
                    log::log!(log_level, "{} was cancelled via panic", log_template);

                    if !is_critical {
                        return FutureOutcome::new(
                            action_name,
                            future_id,
                            CompletionReason::Canceled,
                        );
                    }
                }

                let error_message = format!("{} panicked with error: {}", log_template, error_msg);
                error!("{}", error_message);

                spawn_graceful_shutdown(&log_template, &error_message);

                FutureOutcome::new(action_name, future_id, CompletionReason::Panicked)
            }
            None => {
                let error_message = format!("{} panicked with non string error", log_template);
                error!("{}", error_message);

                spawn_graceful_shutdown(&log_template, &error_message);

                FutureOutcome::new(action_name, future_id, CompletionReason::Panicked)
            }
        },
    }
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
    use crate::core::lifecycle::cancellation_token::CancellationToken;

    use super::*;
    use anyhow::{bail, Result};
    use futures::FutureExt;

    #[tokio::test]
    async fn future_completed_successfully() -> Result<()> {
        // Arrange
        let action = async { Ok(()) };

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(
            future_outcome.completion_reason,
            CompletionReason::CompletedSuccessfully
        );

        Ok(())
    }

    #[tokio::test]
    async fn future_canceled_via_result() -> Result<()> {
        // Arrange
        let action = async { bail!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn future_error() -> Result<()> {
        // Arrange
        let action = async { bail!("Some error") };

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Error);

        Ok(())
    }

    #[tokio::test]
    async fn non_critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_future("test_action_name", false, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Panicked);

        Ok(())
    }

    #[tokio::test]
    async fn panic_with_application_manager() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        let application_manager = ApplicationManager::new(CancellationToken::new());
        keep_application_manager(application_manager);

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed()).await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Panicked);

        Ok(())
    }

    #[tokio::test]
    async fn future_aborted() {
        // Arrange
        let test_value = Arc::new(Mutex::new(false));
        let test_to_future = test_value.clone();
        let action = async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            *test_to_future.lock() = true;

            Ok(())
        };

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed());
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
            let future_outcome = spawn_future_timed(
                "test_action_name",
                true,
                Duration::from_secs(0),
                action.boxed(),
            )
            .await?;

            // Assert
            assert_eq!(
                future_outcome.completion_reason,
                CompletionReason::TimeExpired
            );

            Ok(())
        }

        #[tokio::test]
        async fn error_in_action() -> Result<()> {
            // Arrange
            let action = async { bail!("Some error for test") };

            // Act
            let future_outcome = spawn_future_timed(
                "test_action_name",
                true,
                Duration::from_millis(200),
                action.boxed(),
            )
            .await?;

            // Assert
            assert_eq!(future_outcome.completion_reason, CompletionReason::Error);

            Ok(())
        }

        #[tokio::test]
        async fn action_completed_in_time() -> Result<()> {
            // Arrange
            let action = async { Ok(()) };

            // Act
            let future_outcome = spawn_future_timed(
                "test_action_name",
                true,
                Duration::from_millis(200),
                action.boxed(),
            )
            .await?;

            // Assert
            assert_eq!(
                future_outcome.completion_reason,
                CompletionReason::CompletedSuccessfully
            );

            Ok(())
        }

        #[tokio::test]
        async fn timed_future_aborted() {
            // Arrange
            let test_value = Arc::new(Mutex::new(false));
            let test_to_future = test_value.clone();
            let action = async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                *test_to_future.lock() = true;

                Ok(())
            };

            // Act
            let future_outcome = spawn_future_timed(
                "test_action_name",
                true,
                Duration::from_millis(200),
                action.boxed(),
            );
            future_outcome.abort();
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Assert
            assert_eq!(*test_value.lock(), false);
        }
    }
}

pub trait WithExpect<T> {
    /// Unwrap the value or panic with additional context that is evaluated lazily
    /// only for None variant
    fn with_expect<C, F>(self, f: F) -> T
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T> WithExpect<T> for Option<T> {
    fn with_expect<C, F>(self, f: F) -> T
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.unwrap_or_else(|| panic!("{}", f()))
    }
}
