use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Default)]
struct CancellationState {
    signal: Notify,
    is_cancellation_requested: AtomicBool,
}

/// Lightweight object for signalling about cancellation of operation
/// Note: expected passing through methods by owning value with cloning if we need checking cancellation in many places
#[derive(Default, Clone)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_linked_token(token: &CancellationToken) -> Self {
        token.clone()
    }

    pub fn cancel(&self) {
        let state = &self.state;
        state
            .is_cancellation_requested
            .store(true, Ordering::SeqCst);
        state.signal.notify_waiters();
    }

    /// Returns true if cancellation requested, otherwise false
    pub fn check_cancellation_requested(&self) -> bool {
        self.state.is_cancellation_requested.load(Ordering::SeqCst)
    }

    pub async fn when_cancelled(&self) {
        self.state.clone().signal.notified().await;
    }
}

#[cfg(test)]
mod tests {
    use crate::core::exchanges::cancellation_token::CancellationToken;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use tokio::time::Duration;

    #[test]
    fn just_cancel() {
        let token = CancellationToken::new();
        assert_eq!(token.check_cancellation_requested(), false);

        token.cancel();
        assert_eq!(token.check_cancellation_requested(), true);
    }

    #[tokio::test]
    async fn single_await() {
        let token = CancellationToken::new();

        let signal = Arc::new(Mutex::new(false));

        spawn_working_future(signal.clone(), token.clone());

        // make sure that we don't complete test too fast accidentally before method when_cancelled() completed
        tokio::time::sleep(Duration::from_millis(2)).await;

        assert_eq!(*signal.lock(), false);
        assert_eq!(token.check_cancellation_requested(), false);

        token.cancel();

        // we need a little wait while spawned `working_future` react for cancellation
        tokio::task::yield_now().await;

        assert_eq!(*signal.lock(), true);
        assert_eq!(token.check_cancellation_requested(), true);
    }

    #[tokio::test]
    async fn many_awaits() {
        let token = CancellationToken::new();

        let signal1 = Arc::new(Mutex::new(false));
        let signal2 = Arc::new(Mutex::new(false));

        spawn_working_future(signal1.clone(), token.clone());
        spawn_working_future(signal2.clone(), token.clone());

        // make sure that we don't complete test too fast accidentally before method when_cancelled() completed
        tokio::time::sleep(Duration::from_millis(2)).await;

        assert_eq!(*signal1.lock(), false);
        assert_eq!(*signal2.lock(), false);
        assert_eq!(token.check_cancellation_requested(), false);

        token.cancel();

        // we need a little wait while spawned `working_future` react for cancellation
        tokio::task::yield_now().await;

        assert_eq!(*signal1.lock(), true);
        assert_eq!(*signal2.lock(), true);
        assert_eq!(token.check_cancellation_requested(), true);
    }

    #[test]
    fn double_cancel_call() {
        let token = CancellationToken::new();
        assert_eq!(token.check_cancellation_requested(), false);

        token.cancel();
        assert_eq!(token.check_cancellation_requested(), true);

        token.cancel();
        assert_eq!(token.check_cancellation_requested(), true);
    }

    fn spawn_working_future(signal: Arc<Mutex<bool>>, token: CancellationToken) {
        let _ = tokio::spawn(async move {
            token.when_cancelled().await;
            *signal.lock() = true;
        });
    }
}
