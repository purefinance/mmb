#[cfg(test)]
use mockall::automock;
/// If you'll use this mod in some tests, mocks object should be created.
/// Automock doesn't support default implementation.
/// NOTE: you need to avoid using mock objects in a parallel way https://docs.rs/mockall/0.10.2/mockall/#static-methods
///       some example is here https://stackoverflow.com/questions/51694017/how-can-i-avoid-running-some-tests-in-parallel
#[cfg_attr(test, automock)]
pub(crate) mod time_manager {

    use crate::core::DateTime;

    /// Return current date in UTC
    pub(crate) fn now() -> DateTime {
        chrono::Utc::now()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;
    use mockall_double::double;
    use once_cell::sync::Lazy;
    use parking_lot::{Mutex, MutexGuard};

    #[double]
    use super::time_manager;

    /// Needs for syncing mock objects https://docs.rs/mockall/0.10.2/mockall/#static-methods
    static TIME_MANAGER_MOCK_MUTEX: Lazy<Mutex<()>> = Lazy::new(Mutex::default);

    pub(crate) fn init_mock(
        seconds_offset: Arc<Mutex<u32>>,
    ) -> (time_manager::__now::Context, MutexGuard<'static, ()>) {
        let mock_locker = TIME_MANAGER_MOCK_MUTEX.lock();
        let time_manager_mock_object = time_manager::now_context();
        time_manager_mock_object.expect().returning(move || {
            chrono::Utc
                .ymd(2021, 9, 20)
                .and_hms(0, 0, seconds_offset.lock().clone())
        });

        (time_manager_mock_object, mock_locker)
    }
}
