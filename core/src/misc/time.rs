#[cfg(test)]
use mockall::automock;

/// If you'll use this mod in some tests, mocks object should be created.
/// Automock doesn't support default implementation.
/// NOTE: you need to avoid using mock objects in a parallel way https://docs.rs/mockall/0.10.2/mockall/#static-methods
///       some example is here https://stackoverflow.com/questions/51694017/how-can-i-avoid-running-some-tests-in-parallel
#[cfg_attr(test, automock)]
pub mod time_manager {

    use mmb_utils::DateTime;

    /// Return current date in UTC
    pub fn now() -> DateTime {
        chrono::Utc::now()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;
    use mockall_double::double;
    use parking_lot::{Mutex, ReentrantMutexGuard};

    #[double]
    use super::time_manager;

    pub(crate) fn init_mock(
        seconds_offset: Arc<Mutex<u32>>,
    ) -> (
        time_manager::__now::Context,
        ReentrantMutexGuard<'static, ()>,
    ) {
        let mock_locker = crate::MOCK_MUTEX.lock();
        let time_manager_mock_object = time_manager::now_context();
        time_manager_mock_object.expect().returning(move || {
            chrono::Utc
                .ymd(2021, 9, 20)
                .and_hms(0, 0, seconds_offset.lock().clone())
        });

        (time_manager_mock_object, mock_locker)
    }
}
