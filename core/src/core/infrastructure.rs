use futures::future::BoxFuture;
use mmb_utils::infrastructure::CustomSpawnFuture;
use mmb_utils::infrastructure::FutureOutcome;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::panic;
use std::sync::Arc;
use std::{pin::Pin, time::Duration};
use tokio::task::JoinHandle;

use super::lifecycle::application_manager::ApplicationManager;

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

/// Spawn future with timer. Error will be logged if times up before action completed
/// Other nuances are the same as spawn_future()
pub fn spawn_future_timed(
    action_name: &str,
    is_critical: bool,
    duration: Duration,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    mmb_utils::infrastructure::spawn_future_timed(
        action_name,
        is_critical,
        duration,
        action,
        spawn_graceful_shutdown,
    )
}

/// Spawn future with logging and error, panic and cancellation handling
/// Inside the crate prefer this function to all others
pub fn spawn_future(
    action_name: &str,
    is_critical: bool,
    action: Pin<CustomSpawnFuture>,
) -> JoinHandle<FutureOutcome> {
    mmb_utils::infrastructure::spawn_future(
        action_name,
        is_critical,
        action,
        spawn_graceful_shutdown,
    )
}

fn spawn_graceful_shutdown(log_template: String, error_message: String) {
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
    mmb_utils::infrastructure::spawn_by_timer(
        callback,
        name,
        delay,
        period,
        is_critical,
        spawn_graceful_shutdown,
    )
}

#[cfg(test)]
mod test {
    use mmb_utils::{cancellation_token::CancellationToken, OPERATION_CANCELED_MSG};

    use super::*;
    use anyhow::Result;
    use futures::FutureExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panic_with_application_manager() -> Result<()> {
        // Arrange
        let action = async { panic!("{}", OPERATION_CANCELED_MSG) };

        let application_manager = ApplicationManager::new(CancellationToken::new());
        keep_application_manager(application_manager);

        // Act
        let future_outcome = spawn_future("test_action_name", true, action.boxed()).await?;

        // Assert
        assert!(future_outcome
            .into_result()
            .expect_err("in test")
            .to_string()
            .contains("panicked"));

        Ok(())
    }
}
