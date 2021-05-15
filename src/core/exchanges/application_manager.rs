use super::cancellation_token::CancellationToken;
use crate::core::lifecycle::trading_engine::EngineContext;
use log::{error, info, warn};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

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

    pub async fn start_graceful_shutdown(&self, reason: &str) {
        let engine_context_guard = self.engine_context.lock().await;
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
    }
}
