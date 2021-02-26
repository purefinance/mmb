use enum_map::{Enum, EnumMap};
use parking_lot::Mutex;
use std::fmt::{self, Debug, Formatter};
use tokio::sync::broadcast::{self, Sender};

pub struct EnumAsyncEvent<TEnum: Enum<Mutex<Sender<TResult>>>, TResult: Sized + Clone> {
    map: EnumMap<TEnum, Mutex<Sender<TResult>>>,
}

impl<TResult: Sized + Clone, TEnum: Enum<Mutex<Sender<TResult>>>> EnumAsyncEvent<TEnum, TResult> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn raise_event(&self, waiting_case: TEnum, result: TResult) {
        let sender_guard = &mut self.map[waiting_case].lock();
        let _ = sender_guard.send(result);
        **sender_guard = Self::create_sender();
    }

    pub async fn wait(&self, waiting_case: TEnum) -> TResult {
        let mut receiver = self.map[waiting_case].lock().subscribe();
        receiver
            .recv()
            .await
            .expect("EnumAsyncEvent receiver should not fail because we send only 1 message")
    }

    fn create_sender() -> broadcast::Sender<TResult> {
        broadcast::channel(1).0
    }
}

impl<TEnum, TResult> Default for EnumAsyncEvent<TEnum, TResult>
where
    TEnum: Enum<Mutex<Sender<TResult>>>,
    TResult: Sized + Clone,
{
    fn default() -> Self {
        EnumAsyncEvent {
            map: EnumMap::from(|_| Mutex::new(Self::create_sender())),
        }
    }
}

impl<TEnum, TResult> Debug for EnumAsyncEvent<TEnum, TResult>
where
    TEnum: Enum<Mutex<Sender<TResult>>> + Debug,
    TResult: Sized + Clone + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.map.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::async_event::EnumAsyncEvent;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn raise_events() {
        let event = Arc::new(EnumAsyncEvent::new());

        let signal = Arc::new(Mutex::new((0, 0)));
        spawn_event_waiter(1, signal.clone(), event.clone()).await;
        spawn_event_waiter(2, signal.clone(), event.clone()).await;

        tokio::time::sleep(Duration::from_millis(2)).await;

        assert_eq!(*signal.lock(), (0, 0));

        event.raise_event(2, 17);

        tokio::task::yield_now().await;
        assert_eq!(*signal.lock(), (2, 17));
        spawn_event_waiter(2, signal.clone(), event.clone()).await;

        event.raise_event(1, 15);

        tokio::task::yield_now().await;
        assert_eq!(*signal.lock(), (1, 15));
        spawn_event_waiter(1, signal.clone(), event.clone()).await;

        event.raise_event(1, 19);

        tokio::task::yield_now().await;
        assert_eq!(*signal.lock(), (1, 19));
        spawn_event_waiter(1, signal.clone(), event.clone()).await;

        event.raise_event(2, 12);

        tokio::task::yield_now().await;
        assert_eq!(*signal.lock(), (2, 12));
    }

    #[tokio::test]
    async fn dont_trigger_same_handler_second_time() {
        let event = Arc::new(EnumAsyncEvent::new());

        let signal = Arc::new(Mutex::new((0, 0)));
        spawn_event_waiter(1, signal.clone(), event.clone()).await;

        event.raise_event(1, 15);

        tokio::task::yield_now().await;
        assert_eq!(*signal.lock(), (1, 15));

        event.raise_event(1, 19);

        assert_eq!(*signal.lock(), (1, 15));
    }

    async fn spawn_event_waiter(
        waiting_case: u8,
        signal: Arc<Mutex<(u8, u32)>>,
        event: Arc<EnumAsyncEvent<u8, u32>>,
    ) {
        tokio::spawn(async move {
            let result = event.wait(waiting_case).await;
            *signal.lock() = (waiting_case, result);
        });
        tokio::task::yield_now().await;
    }
}
