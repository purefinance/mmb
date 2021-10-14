use std::{
    sync::atomic::AtomicU64,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn get_current_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Unable to get time since unix epoch started")
        .as_millis()
}

// Should be used for unique IDs based on current time
pub(crate) fn get_atomic_current_secs() -> AtomicU64 {
    AtomicU64::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get system time since UNIX_EPOCH")
            .as_secs(),
    )
}
