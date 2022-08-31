use std::sync::atomic::AtomicU64;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::infrastructure::WithExpect;
use crate::DateTime;

pub fn u64_to_date_time(src: u64) -> DateTime {
    (UNIX_EPOCH + Duration::from_millis(src)).into()
}

pub fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

/// Function should be used for initialization of unique IDs based on incrementing AtomicU64 counter.
/// Returned value initialized with current UNIX time.
/// # Example:
/// ```ignore
/// use once_cell::sync::Lazy;
/// use std::sync::atomic::{AtomicU64, Ordering};
///
/// static CLIENT_ORDER_ID_COUNTER: Lazy<AtomicU64> = Lazy::new(|| get_atomic_current_secs());
///
/// let new_id = CLIENT_ORDER_ID_COUNTER.fetch_add(1, Ordering::AcqRel);
/// ```
pub fn get_atomic_current_secs() -> AtomicU64 {
    AtomicU64::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get system time since UNIX_EPOCH")
            .as_secs(),
    )
}

pub trait ToStdExpected {
    fn to_std_expected(&self) -> Duration;
}

impl ToStdExpected for chrono::Duration {
    /// Converts chrono::Duration to std::time::Duration.
    ///
    /// # Panics
    /// Panic only on negative delay
    fn to_std_expected(&self) -> Duration {
        self.to_std().with_expect(|| {
            format!("Unable to convert {self} from chrono::Duration to std::time::Duration")
        })
    }
}
