use super::cancellation_token::CancellationToken;
use crate::core::{lifecycle::trading_engine::EngineContext, utils::unset_application_manager};
use log::{error, info, warn};
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
    pub fn spawn_graceful_shutdown(self: Arc<Self>, reason: String) -> JoinHandle<()> {
        let handler = tokio::spawn(async move {
            let engine_context_guard = match self.engine_context.try_lock() {
                Ok(engine_context_guard) => engine_context_guard,
                Err(_) => {
                    // if we can't acquire lock, it mean's that someone another acquire lock and will invoke graceful shutdown or it was already invoked
                    // Return to not hold tasks that should be finished as soon as EngineContext::is_graceful_shutdown_started should be true
                    return;
                }
            };

            start_graceful_shutdown_inner(engine_context_guard, &reason).await
        });

        Self::handle_possible_panic(handler)
    }

    fn handle_possible_panic(graceful_shutdown_handler: JoinHandle<()>) -> JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(error) = graceful_shutdown_handler.await {
                if error.is_panic() {
                    let panic = error.into_panic();
                    let maybe_panic_msg = panic.as_ref().downcast_ref::<String>().clone();
                    match maybe_panic_msg {
                        None => {
                            error!("Graceful shutdown future panicked without message")
                        }
                        Some(panic_msg) => {
                            error!("Graceful shutdown future panicked: {:?}", panic_msg)
                        }
                    }
                }
            }
        })
    }

    /// Launch async graceful shutdown operation
    pub async fn run_graceful_shutdown(&self, reason: &str) {
        let engine_context_guard = self.engine_context.lock().await;
        start_graceful_shutdown_inner(engine_context_guard, reason).await;
    }
}

pub async fn start_graceful_shutdown_inner(
    engine_context_guard: MutexGuard<'_, Option<Weak<EngineContext>>>,
    reason: &str,
) {
    let engine_context = match &*engine_context_guard {
        Some(ctx) => ctx,
        None => {
            error!("Tried to request graceful shutdown with reason '{}', but 'engine_context' is not specified", reason);
            return;
        }
    };

    info!("Requested graceful shutdown: {}", reason);

    match engine_context.upgrade() {
        None => warn!("Can't execute graceful shutdown with reason '{}', because 'engine_context' was dropped already", reason),
        Some(ctx) => ctx.graceful_shutdown().await,
    }

    unset_application_manager();
}
