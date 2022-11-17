use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use tokio::sync::Notify;

use crate::nothing_to_do;
use crate::OPERATION_CANCELED_MSG;

#[derive(Default)]
struct CancellationState {
    signal: Notify,
    handlers: Mutex<Vec<Box<dyn Fn() + Send>>>,
    is_cancellation_requested: AtomicBool,
}

/// Lightweight object for signalling about cancellation of operation
/// NOTE: expected passing through methods by owning value with cloning if we need checking cancellation in many places
#[derive(Default, Clone)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        let state = &self.state;
        state
            .is_cancellation_requested
            .store(true, Ordering::SeqCst);

        state.handlers.lock().iter().for_each(|handler| handler());
        state.signal.notify_waiters();
    }

    /// Returns true if cancellation requested, otherwise false
    pub fn is_cancellation_requested(&self) -> bool {
        self.state.is_cancellation_requested.load(Ordering::SeqCst)
    }

    /// Returns Result::Err() if cancellation requested, otherwise Ok(())
    pub fn error_if_cancellation_requested(&self) -> Result<()> {
        match self.is_cancellation_requested() {
            true => bail!(OPERATION_CANCELED_MSG),
            false => Ok(()),
        }
    }

    pub async fn when_cancelled(&self) {
        let action = async {
            if self.is_cancellation_requested() {
                return;
            }

            std::future::pending::<()>().await;
        };

        tokio::select! {
            _ = self.state.signal.notified() => nothing_to_do(),
            _ = action => nothing_to_do(),
        };
    }

    pub fn create_linked_token(&self) -> Self {
        let new_token = CancellationToken::new();

        {
            let weak_cancellation = Arc::downgrade(&new_token.state);
            self.register_handler(Box::new(move || match weak_cancellation.upgrade() {
                None => nothing_to_do(),
                Some(state) => CancellationToken { state }.cancel(),
            }))
        }

        if self.is_cancellation_requested() {
            new_token.cancel();
        }

        new_token
    }

    fn register_handler(&self, handler: Box<dyn Fn() + Send>) {
        self.state.handlers.lock().push(handler);
    }
}

#[cfg(test)]
mod tests {
    use crate::cancellation_token::CancellationToken;
    use crate::infrastructure::with_timeout;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use tokio::time::Duration;

    #[test]
    fn just_cancel() {
        let token = CancellationToken::new();
        assert!(!token.is_cancellation_requested());

        token.cancel();
        assert!(token.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn single_await() {
        let token = CancellationToken::new();

        // make sure that we don't complete test too fast accidentally before method when_cancelled() completed
        tokio::time::sleep(Duration::from_millis(2)).await;

        assert!(!token.is_cancellation_requested());

        token.cancel();

        let max_timeout = Duration::from_secs(2);
        with_timeout(max_timeout, token.when_cancelled()).await;

        assert!(token.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn many_awaits() {
        let token = CancellationToken::new();
        let token1 = token.clone();
        let token2 = token.clone();

        let signal1 = Arc::new(Mutex::new(false));
        let signal2 = Arc::new(Mutex::new(false));

        // make sure that we don't complete test too fast accidentally before method when_cancelled() completed
        tokio::time::sleep(Duration::from_millis(2)).await;

        assert!(!*signal1.lock());
        assert!(!*signal2.lock());
        assert!(!token.is_cancellation_requested());

        token.cancel();

        let max_timeout = Duration::from_secs(2);
        with_timeout(max_timeout, token1.when_cancelled()).await;
        with_timeout(max_timeout, token2.when_cancelled()).await;

        assert!(token.is_cancellation_requested());
    }

    #[test]
    fn double_cancel_call() {
        let token = CancellationToken::new();
        assert!(!token.is_cancellation_requested());

        token.cancel();
        assert!(token.is_cancellation_requested());

        token.cancel();
        assert!(token.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_source_token_when_linked_source_token_is_not_cancelled() {
        let source_token = CancellationToken::new();
        assert!(!source_token.is_cancellation_requested());

        let new_token = source_token.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token.is_cancellation_requested());

        source_token.cancel();
        assert!(source_token.is_cancellation_requested());
        assert!(new_token.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn create_linked_token_when_source_token_is_cancelled() {
        let source_token = CancellationToken::new();
        source_token.cancel();
        assert!(source_token.is_cancellation_requested());

        let new_token = source_token.create_linked_token();
        assert!(source_token.is_cancellation_requested());
        assert!(new_token.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_new_linked_token_when_source_token_is_not_cancelled() {
        let source_token = CancellationToken::new();
        assert!(!source_token.is_cancellation_requested());

        let new_token = source_token.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token.is_cancellation_requested());

        new_token.cancel();
        assert!(!source_token.is_cancellation_requested());
        assert!(new_token.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_when_2_new_linked_tokens_to_single_source() {
        // source -> token1
        //      \--> token2

        let source_token = CancellationToken::new();
        assert!(!source_token.is_cancellation_requested());

        let new_token1 = source_token.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token1.is_cancellation_requested());

        let new_token2 = source_token.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token1.is_cancellation_requested());
        assert!(!new_token2.is_cancellation_requested());

        source_token.cancel();
        assert!(source_token.is_cancellation_requested());
        assert!(new_token1.is_cancellation_requested());
        assert!(new_token2.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_source_when_2_sequentially_new_linked_tokens() {
        // source -> token1 -> token2
        let source_token = CancellationToken::new();
        assert!(!source_token.is_cancellation_requested());

        let new_token1 = source_token.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token1.is_cancellation_requested());

        let new_token2 = new_token1.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token1.is_cancellation_requested());
        assert!(!new_token2.is_cancellation_requested());

        source_token.cancel();
        assert!(source_token.is_cancellation_requested());
        assert!(new_token1.is_cancellation_requested());
        assert!(new_token2.is_cancellation_requested());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_token1_when_2_sequentially_new_linked_tokens() {
        // source -> token1 -> token2
        let source_token = CancellationToken::new();
        assert!(!source_token.is_cancellation_requested());

        let new_token1 = source_token.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token1.is_cancellation_requested());

        let new_token2 = new_token1.create_linked_token();
        assert!(!source_token.is_cancellation_requested());
        assert!(!new_token1.is_cancellation_requested());
        assert!(!new_token2.is_cancellation_requested());

        new_token1.cancel();
        assert!(!source_token.is_cancellation_requested());
        assert!(new_token1.is_cancellation_requested());
        assert!(new_token2.is_cancellation_requested());
    }
}
