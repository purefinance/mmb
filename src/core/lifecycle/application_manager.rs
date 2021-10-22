use super::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::EngineContext;
use crate::core::nothing_to_do;
use futures::{Future, FutureExt};
use log::{error, info, warn};
use std::panic;
use std::sync::{Arc, Weak};
use tokio::sync::{Mutex, MutexGuard};
use tokio::task::JoinHandle;

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

        let graceful_shutdown_handler =
            start_graceful_shutdown_inner(engine_context_guard, &reason)?;
        Some(tokio::spawn(async move {
            let action_outcome = panic::AssertUnwindSafe(graceful_shutdown_handler)
                .catch_unwind()
                .await;
            let future_name = "Graceful shutdown future";
            match action_outcome {
                Ok(()) => {
                    info!("{} completed successfully", future_name);
                }
                Err(panic) => match panic.as_ref().downcast_ref::<String>() {
                    Some(panic_message) => {
                        error!("{} panicked with error: {}", future_name, panic_message);
                    }
                    None => {
                        error!("{} panicked without message", future_name);
                    }
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
    let engine_context = match &*engine_context_guard {
        Some(ctx) => ctx,
        None => {
            error!("Tried to request graceful shutdown with reason '{}', but 'engine_context' is not specified", reason);
            return None;
        }
    };

    info!("Requested graceful shutdown: {}", reason);

    match engine_context.upgrade() {
        None => {
            warn!("Can't execute graceful shutdown with reason '{}', because 'engine_context' was dropped already", reason);
            None
        }
        Some(ctx) => Some(ctx.graceful_shutdown()),
    }
}
