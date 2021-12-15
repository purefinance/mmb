use std::time::{Duration, UNIX_EPOCH};

use crate::DateTime;

pub fn u64_to_date_time(src: u64) -> DateTime {
    (UNIX_EPOCH + Duration::from_millis(src)).into()
}
