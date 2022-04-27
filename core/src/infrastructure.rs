use anyhow::Result;
use futures::Future;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::FutureOutcome;
use mmb_utils::infrastructure::SpawnFutureFlags;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::panic;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use super::lifecycle::app_lifetime_manager::AppLifetimeManager;

static LIFETIME_MANAGER: OnceCell<Mutex<Option<Arc<AppLifetimeManager>>>> = OnceCell::new();

pub fn init_lifetime_manager() -> Arc<AppLifetimeManager> {
    let manger = AppLifetimeManager::new(CancellationToken::new());
    keep_lifetime_manager(manger.clone());

    manger
}

pub(crate) fn keep_lifetime_manager(lifetime_manager: Arc<AppLifetimeManager>) {
    let mut lifetime_manager_guard = LIFETIME_MANAGER
        .get_or_init(|| Mutex::new(Some(lifetime_manager.clone())))
        .lock();

    *lifetime_manager_guard = Some(
        lifetime_manager_guard
            .as_ref()
            .unwrap_or(&lifetime_manager)
            .clone(),
    );
}

pub(crate) fn unset_lifetime_manager() {
    match LIFETIME_MANAGER.get() {
        Some(lifetime_manager) => lifetime_manager.lock().take(),
        None => panic!(
            "Attempt to unset static application manager for spawn_future() before it has been set"
        ),
    };
}

fn get_futures_cancellation_token() -> CancellationToken {
    LIFETIME_MANAGER
        .get()
        .expect("Unable to get_futures_cancellation_token if AppLifetimeManager isn't set")
        .lock()
        .as_ref()
        .expect("AppLifetimeManager is none")
        .futures_cancellation_token
        .clone()
}

/// Spawn future with timer. Error will be logged if times up before action completed
/// Other nuances are the same as spawn_future()
pub fn spawn_future_timed(
    action_name: &str,
    flags: SpawnFutureFlags,
    duration: Duration,
    action: impl Future<Output = Result<()>> + Send + 'static,
) -> JoinHandle<FutureOutcome> {
    mmb_utils::infrastructure::spawn_future_timed(
        action_name,
        flags,
        duration,
        action,
        spawn_graceful_shutdown,
        get_futures_cancellation_token(),
    )
}

pub fn spawn_future_ok(
    action_name: &str,
    flags: SpawnFutureFlags,
    action: impl Future<Output = ()> + Send + 'static,
) -> JoinHandle<FutureOutcome> {
    spawn_future(action_name, flags, async move {
        action.await;
        Ok(())
    })
}

/// Spawn future with logging and error, panic and cancellation handling
/// Inside the crate prefer this function to all others
pub fn spawn_future(
    action_name: &str,
    flags: SpawnFutureFlags,
    action: impl Future<Output = Result<()>> + Send + 'static,
) -> JoinHandle<FutureOutcome> {
    mmb_utils::infrastructure::spawn_future(
        action_name,
        flags,
        action,
        spawn_graceful_shutdown,
        get_futures_cancellation_token(),
    )
}

fn spawn_graceful_shutdown(log_template: String, error_message: String) {
    match LIFETIME_MANAGER.get() {
        Some(lifetime_manager) => {
            match &*lifetime_manager.lock() {
                Some(lifetime_manager) => {
                    lifetime_manager.spawn_graceful_shutdown(error_message);
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
pub fn spawn_by_timer<F, Fut>(
    callback: F,
    name: &str,
    delay: Duration,
    period: Duration,
    flags: SpawnFutureFlags,
) -> JoinHandle<FutureOutcome>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    mmb_utils::infrastructure::spawn_by_timer(
        callback,
        name,
        delay,
        period,
        flags,
        get_futures_cancellation_token(),
        spawn_graceful_shutdown,
    )
}

#[cfg(test)]
mod test {
    use mmb_utils::{cancellation_token::CancellationToken, OPERATION_CANCELED_MSG};

    use super::*;
    use anyhow::Result;
    use mmb_utils::infrastructure::init_infrastructure;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panic_with_deny_cancellation() -> Result<()> {
        init_infrastructure("log.txt");
        // Arrange
        let manager = AppLifetimeManager::new(CancellationToken::new());
        keep_lifetime_manager(manager);

        // Act
        let future_outcome = spawn_future(
            "test_action_name",
            SpawnFutureFlags::DENY_CANCELLATION | SpawnFutureFlags::STOP_BY_TOKEN,
            async { panic!("{}", OPERATION_CANCELED_MSG) },
        )
        .await?
        .into_result()
        .expect_err("in test")
        .to_string();

        // Assert
        assert!(future_outcome.contains("panicked"));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn panic_without_deny_cancellation() -> Result<()> {
        init_infrastructure("log.txt");
        // Arrange
        let application_manager = AppLifetimeManager::new(CancellationToken::new());
        keep_lifetime_manager(application_manager);

        // Act
        let future_outcome =
            spawn_future("test_action_name", SpawnFutureFlags::STOP_BY_TOKEN, async {
                panic!("{}", OPERATION_CANCELED_MSG)
            })
            .await?
            .into_result()
            .expect_err("in test")
            .to_string();

        // Assert
        assert!(future_outcome.contains("canceled"));

        Ok(())
    }
}
