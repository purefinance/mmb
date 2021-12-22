use futures::{Future, FutureExt};
use mmb_utils::nothing_to_do;
use tokio::sync::{Mutex, MutexGuard};
use tokio::task::JoinHandle;

use std::panic;
use std::sync::{Arc, Weak};

use crate::core::lifecycle::trading_engine::EngineContext;

use mmb_utils::cancellation_token::CancellationToken;

pub struct ApplicationManager {
    cancellation_token: CancellationToken,
    engine_context: Mutex<Option<Weak<EngineContext>>>,
}

impl ApplicationManager {
    pub fn new(cancellation_token: CancellationToken) -> Arc<Self> {
        Arc::new(Self {
            cancellation_token,
            engine_context: Mutex::new(None),
        })
    }

    pub fn stop_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    pub(crate) fn setup_engine_context(&self, engine_context: Arc<EngineContext>) {
        let mut engine_context_guard = self
            .engine_context
            .try_lock()
            .expect("method should be invoked just after creation when there are no aliases");
        *engine_context_guard = Some(Arc::downgrade(&engine_context));
    }

    /// Synchronous method for starting graceful shutdown with blocking current thread and
    /// without waiting for the operation to complete
    pub fn spawn_graceful_shutdown(self: Arc<Self>, reason: String) -> Option<JoinHandle<()>> {
        let engine_context_guard = match self.engine_context.try_lock() {
            Ok(engine_context_guard) => engine_context_guard,
            // if we can't acquire lock, it mean's that someone another acquire lock and will invoke graceful shutdown or it was already invoked
            // Return to not hold tasks that should be finished as soon as EngineContext::is_graceful_shutdown_started should be true
            Err(_) => return None,
        };

        let handler = start_graceful_shutdown_inner(engine_context_guard, &reason)?;

        Some(tokio::spawn(async move {
            static FUTURE_NAME: &str = "Graceful shutdown future";

            let action_outcome = panic::AssertUnwindSafe(handler).catch_unwind().await;

            match action_outcome {
                Ok(()) => log::info!("{} completed successfully", FUTURE_NAME),
                Err(panic) => match panic.downcast_ref::<String>() {
                    Some(message) => {
                        log::error!("{} panicked with error: {}", FUTURE_NAME, message)
                    }
                    None => log::error!("{} panicked without message", FUTURE_NAME),
                },
            }
        }))
    }

    /// Launch async graceful shutdown operation
    pub async fn run_graceful_shutdown(&self, reason: &str) {
        let engine_context_guard = self.engine_context.lock().await;
        let fut_opt = start_graceful_shutdown_inner(engine_context_guard, reason);
        match fut_opt {
            None => nothing_to_do(),
            Some(fut) => fut.await,
        }
    }
}

pub fn start_graceful_shutdown_inner(
    engine_context_guard: MutexGuard<'_, Option<Weak<EngineContext>>>,
    reason: &str,
) -> Option<impl Future<Output = ()> + 'static> {
    let engine_context = engine_context_guard.as_ref().or_else(|| {
        log::error!("Tried to request graceful shutdown with reason '{}', but 'engine_context' is not specified", reason);
        None
    })?;

    log::info!("Requested graceful shutdown: {}", reason);

    match engine_context.upgrade() {
        None => {
            log::warn!("Can't execute graceful shutdown with reason '{}', because 'engine_context' was dropped already", reason);
            None
        }
        Some(ctx) => Some(ctx.graceful_shutdown()),
    }
}
