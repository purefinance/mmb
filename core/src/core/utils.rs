use std::sync::atomic::AtomicU64;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn get_current_milliseconds() -> u128 {
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
