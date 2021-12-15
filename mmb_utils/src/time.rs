use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
