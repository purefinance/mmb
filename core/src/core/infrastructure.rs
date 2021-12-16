use anyhow::{bail, Result};
use futures::future::BoxFuture;
use futures::Future;
use futures::FutureExt;
use log::log;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::fmt::{Arguments, Debug, Display};
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

    log::info!("Future {} with id {} started", action_name, future_id);

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                log::error!("Time in form of {:?} is over, but future {} is not completed yet", duration, action_name);
                FutureOutcome::new(action_name, future_id, CompletionReason::TimeExpired)
            }
            action_outcome = action => {
                action_outcome
            }
        }
    })
}

/// Spawn future with logging and error, panic and cancellation handling
/// Inside the crate prefer this function to all others
pub fn spawn_future(
    action_name: &str,
    is_critical: bool,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();

    log::info!("Future {} with id {} started", action_name, future_id);

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
                log::trace!("{} successfully completed", log_template);

                FutureOutcome::new(
                    action_name,
                    future_id,
                    CompletionReason::CompletedSuccessfully,
                )
            }
            Err(error) => {
                if error.to_string() == OPERATION_CANCELED_MSG {
                    log::trace!("{} was cancelled via Result<()>", log_template);

                    return FutureOutcome::new(action_name, future_id, CompletionReason::Canceled);
                }

                log::error!("{} returned error: {:?}", log_template, error);
                return FutureOutcome::new(action_name, future_id, CompletionReason::Error);
            }
        },
        Err(panic) => match panic.as_ref().downcast_ref::<String>() {
            Some(error_msg) => {
                if error_msg == OPERATION_CANCELED_MSG {
                    let log_level = if is_critical {
                        log::Level::Error
                    } else {
                        log::Level::Trace
                    };
                    log!(log_level, "{} was cancelled via panic", log_template);

                    if !is_critical {
                        return FutureOutcome::new(
                            action_name,
                            future_id,
                            CompletionReason::Canceled,
                        );
                    }
                }

                let error_message = format!("{} panicked with error: {}", log_template, error_msg);
                log::error!("{}", error_message);

                spawn_graceful_shutdown(&log_template, &error_message);

                FutureOutcome::new(action_name, future_id, CompletionReason::Panicked)
            }
            None => {
                let error_message = format!("{} panicked with non string error", log_template);
                log::error!("{}", error_message);

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
                None => log::error!("Unable to start graceful shutdown after panic inside {} because there are no application manager",
                    log_template),
            }
        }
        None => log::error!("Unable to start graceful shutdown after panic inside {} because there are no application manager",
            log_template),
    }
}

/// This function spawn a future after waiting for some `delay`
/// and will repeat the `callback` endlessly with some `period`
pub fn spawn_by_timer(
    callback: impl Fn() -> BoxFuture<'static, ()> + Send + Sync + 'static,
    name: &str,
    delay: Duration,
    period: Duration,
    is_critical: bool,
) -> JoinHandle<FutureOutcome> {
    spawn_future(
        name,
        is_critical,
        async move {
            tokio::time::sleep(delay).await;
            loop {
                (callback)().await;
                tokio::time::sleep(period).await;
            }
        }
        .boxed(),
    )
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

        #[tokio::test]
        async fn repetable_action() {
            let counter = Arc::new(Mutex::new(0u64));
            let duration = 200;
            let repeats_count = 5;
            let future_outcome = {
                async fn future(counter: Arc<Mutex<u64>>) {
                    *counter.lock() += 1;
                }

                let counter = counter.clone();
                spawn_by_timer(
                    move || (future)(counter.clone()).boxed(),
                    "spawn_repeatable".into(),
                    Duration::ZERO,
                    Duration::from_millis(duration),
                    true,
                )
            };

            tokio::time::sleep(Duration::from_millis(repeats_count * duration)).await;
            assert_eq!(*counter.lock(), repeats_count);

            future_outcome.abort();
            tokio::time::sleep(Duration::from_millis(repeats_count / 2)).await;
            assert_eq!(*counter.lock(), repeats_count);
        }
    }
}

pub trait WithExpect<T> {
    /// Unwrap the value or panic with additional context that is evaluated lazily
    /// only for None variant
    fn with_expect<C>(self, f: impl FnOnce() -> C) -> T
    where
        C: Display + Send + Sync + 'static;

    /// Unwrap the value or panic with additional context that is evaluated lazily
    /// The performance version. Double memory is not allocated for formatting
    ///
    /// # Examples
    ///```should_panic
    /// use mmb_core::core::infrastructure::WithExpect;
    ///
    /// let result: Result<(), ()> = Err(());
    /// result.with_expect_args(|f| f(&format_args!("Error {}", "Message")));
    /// ```
    fn with_expect_args(self, f: impl FnOnce(&dyn Fn(&Arguments))) -> T;
}

impl<T> WithExpect<T> for Option<T> {
    fn with_expect<C>(self, f: impl FnOnce() -> C) -> T
    where
        C: Display + Send + 'static,
    {
        self.unwrap_or_else(|| panic!("{}", f()))
    }

    fn with_expect_args(self, f: impl FnOnce(&dyn Fn(&Arguments))) -> T {
        self.unwrap_or_else(|| {
            f(&|args| panic!("{}", args));
            unreachable!()
        })
    }
}

impl<T, E> WithExpect<T> for std::result::Result<T, E>
where
    E: Debug,
{
    fn with_expect<C>(self, f: impl FnOnce() -> C) -> T
    where
        C: Display + Send + Sync + 'static,
    {
        match self {
            Ok(v) => v,
            Err(e) => panic!("{}: {:?}", f(), e),
        }
    }

    fn with_expect_args(self, f: impl FnOnce(&dyn Fn(&Arguments))) -> T {
        match self {
            Ok(v) => v,
            Err(e) => {
                f(&|args| panic!("{}: {:?}", args, e));
                unreachable!()
            }
        }
    }
}
