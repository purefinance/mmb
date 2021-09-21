#[cfg(test)]
use mockall::automock;

/// If you'll use this mod in some tests, mocks object should be created.
/// Automock doesn't support default implementation.
/// NOTE: you need to avoid using mock objects in a parallel way https://docs.rs/mockall/0.10.2/mockall/#static-methods
///       some example is here https://stackoverflow.com/questions/51694017/how-can-i-avoid-running-some-tests-in-parallel
#[cfg_attr(test, automock)]
pub(crate) mod time_manager {
    use crate::core::DateTime;

    pub(crate) fn now() -> DateTime {
        chrono::Utc::now()
    }
}
