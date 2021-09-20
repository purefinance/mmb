#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub(crate) mod time_manager {
    use crate::core::DateTime;

    pub(crate) fn now() -> DateTime {
        chrono::Utc::now()
    }
}
