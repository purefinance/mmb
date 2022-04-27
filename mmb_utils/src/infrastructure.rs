use anyhow::{bail, Result};
use bitflags::bitflags;
use futures::Future;
use futures::FutureExt;
use std::fmt::Arguments;
use std::fmt::{Debug, Display};
use std::panic;
use std::time::Duration;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::cancellation_token::CancellationToken;
use crate::logger::init_logger_file_named;
use crate::logger::print_info;
use crate::panic::handle_future_panic;
use crate::panic::set_panic_hook;
use crate::OPERATION_CANCELED_MSG;

bitflags! {
    pub struct SpawnFutureFlags: u32 {
        /// Run graceful shutdown on cancel for this future, assuming some logical error (deny
        /// cancellation).
        const DENY_CANCELLATION = 0b00000001;
        /// If this flag is set the future will be forced to stop at the end of graceful_shutdown
        const STOP_BY_TOKEN = 0b00000010;
    }
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

    pub fn into_result(self) -> Result<()> {
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

/// Spawn future with timer. Error will be logged if times up before action completed
/// Other nuances are the same as spawn_future()
pub fn spawn_future_timed(
    action_name: &str,
    flags: SpawnFutureFlags,
    duration: Duration,
    action: impl Future<Output = Result<()>> + Send + 'static,
    graceful_shutdown_spawner: impl FnOnce(String, String) + 'static + Send,
    cancellation_token: CancellationToken,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();
    let action = handle_action_outcome(
        action_name.clone(),
        future_id,
        flags,
        action,
        graceful_shutdown_spawner,
        cancellation_token,
    );

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
    flags: SpawnFutureFlags,
    action: impl Future<Output = Result<()>> + Send + 'static,
    graceful_shutdown_spawner: impl FnOnce(String, String) + 'static + Send,
    cancellation_token: CancellationToken,
) -> JoinHandle<FutureOutcome> {
    let action_name = action_name.to_owned();
    let future_id = Uuid::new_v4();

    log::info!("Future {action_name} with id {future_id} started");

    tokio::spawn(handle_action_outcome(
        action_name,
        future_id,
        flags,
        action,
        graceful_shutdown_spawner,
        cancellation_token,
    ))
}

async fn handle_action_outcome(
    action_name: String,
    future_id: Uuid,
    flags: SpawnFutureFlags,
    action: impl Future<Output = Result<()>> + Send + 'static,
    graceful_shutdown_spawner: impl FnOnce(String, String),
    cancellation_token: CancellationToken,
) -> FutureOutcome {
    let log_template = format!("Future '{}', with id {}", action_name, future_id);

    let action_outcome = match flags.intersects(SpawnFutureFlags::STOP_BY_TOKEN) {
        true => tokio::select! {
            res = panic::AssertUnwindSafe(action).catch_unwind() => res,
            _ = cancellation_token.when_cancelled() => {
                print_info(format!("{log_template} has been stopped by cancellation_token"));
                Ok(Ok(()))
            },
        },
        false => panic::AssertUnwindSafe(action).catch_unwind().await,
    };

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
                    log::trace!("{} was cancelled due to Result<()>", log_template);

                    return FutureOutcome::new(action_name, future_id, CompletionReason::Canceled);
                }

                log::error!("{} returned error: {:?}", log_template, error);
                FutureOutcome::new(action_name, future_id, CompletionReason::Error)
            }
        },
        Err(panic_info) => {
            let msg = match panic_info.as_ref().downcast_ref::<&'static str>() {
                Some(s) => *s,
                None => match panic_info.as_ref().downcast_ref::<String>() {
                    Some(s) => &s[..],
                    None => "without readable message",
                },
            };

            handle_future_panic(
                action_name,
                future_id,
                flags,
                graceful_shutdown_spawner,
                log_template,
                msg.into(),
            )
        }
    }
}

/// This function spawn a future after waiting for some `delay`
/// and will repeat the `callback` endlessly with some `period`
pub fn spawn_by_timer<F, Fut>(
    callback: F,
    name: &str,
    delay: Duration,
    period: Duration,
    flags: SpawnFutureFlags,
    cancellation_token: CancellationToken,
    graceful_shutdown_spawner: impl FnOnce(String, String) + 'static + Send,
) -> JoinHandle<FutureOutcome>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    spawn_future(
        name,
        flags,
        async move {
            tokio::time::sleep(delay).await;
            loop {
                (callback)().await;
                tokio::time::sleep(period).await;
            }
        },
        graceful_shutdown_spawner,
        cancellation_token,
    )
}

pub async fn with_timeout<T, Fut>(timeout: Duration, fut: Fut) -> T
where
    Fut: Future<Output = T>,
{
    tokio::select! {
        result = fut => result,
        _ = tokio::time::sleep(timeout) => panic!("Timeout {} ms is exceeded", timeout.as_millis()),
    }
}

/// Do not use this in tests because panics will not be logged #448
pub fn init_infrastructure(log_file: &str) {
    set_panic_hook();
    init_logger_file_named(log_file);
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{bail, Result};
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[tokio::test]
    async fn future_completed_successfully() -> Result<()> {
        // Arrange
        let action = async { Ok(()) };

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
            |_, _| {},
            CancellationToken::default(),
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
    async fn future_canceled_via_result() -> Result<()> {
        // Arrange
        let action = async { bail!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
            |_, _| {},
            CancellationToken::default(),
        )
        .await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn future_error() -> Result<()> {
        // Arrange
        let action = async { bail!("Some error") };

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
            |_, _| {},
            CancellationToken::default(),
        )
        .await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Error);

        Ok(())
    }

    #[tokio::test]
    async fn non_critical_future_canceled_via_panic() -> Result<()> {
        set_panic_hook();

        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN,
            action,
            |_, _| {},
            CancellationToken::default(),
        )
        .await?;

        // Assert
        assert_eq!(future_outcome.completion_reason, CompletionReason::Canceled);

        Ok(())
    }

    #[tokio::test]
    async fn critical_future_canceled_via_panic() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
            |_, _| {},
            CancellationToken::default(),
        )
        .await?;

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
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
            |_, _| {},
            CancellationToken::default(),
        );

        future_outcome.abort();
        let _ = future_outcome.await;

        // Assert
        assert_eq!(*test_value.lock(), false);
    }

    #[tokio::test]
    async fn future_canceled() {
        // Arrange
        let test_value = Arc::new(Mutex::new(false));
        let test_to_future = test_value.clone();
        let action = async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            *test_to_future.lock() = true;

            Ok(())
        };
        let cancellation_token = CancellationToken::default();

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
            action,
            |_, _| {},
            cancellation_token.clone(),
        );
        cancellation_token.cancel();

        let _ = future_outcome.await;

        // Assert
        assert_eq!(*test_value.lock(), false);
    }

    mod with_timer {
        use std::sync::Arc;

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
                SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                Duration::from_secs(0),
                action,
                |_, _| {},
                CancellationToken::default(),
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
                SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                Duration::from_millis(200),
                action,
                |_, _| {},
                CancellationToken::default(),
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
                SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                Duration::from_millis(200),
                action,
                |_, _| {},
                CancellationToken::default(),
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
                SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                Duration::from_millis(200),
                action,
                |_, _| {},
                CancellationToken::default(),
            );
            future_outcome.abort();
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Assert
            assert_eq!(*test_value.lock(), false);
        }

        #[tokio::test]
        async fn repeatable_action() {
            let counter = Arc::new(Mutex::new(0u64));
            let duration = 200;
            let repeats_count = 5;
            let future_outcome = {
                async fn future(counter: Arc<Mutex<u64>>) {
                    *counter.lock() += 1;
                }

                let counter = counter.clone();
                spawn_by_timer(
                    move || (future)(counter.clone()),
                    "spawn_repeatable",
                    Duration::ZERO,
                    Duration::from_millis(duration),
                    SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                    CancellationToken::default(),
                    |_, _| {},
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
    /// use mmb_utils::infrastructure::WithExpect;
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
