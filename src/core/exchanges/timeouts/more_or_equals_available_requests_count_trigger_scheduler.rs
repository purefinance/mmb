use chrono::{Duration, Utc};

use crate::core::DateTime;

pub struct MoreOrEquelsAvailableRequestsCountTriggerScheduler {
    //increasing_count_triggers:
}

impl MoreOrEquelsAvailableRequestsCountTriggerScheduler {
    pub fn schedule_triggers(
        &self,
        available_requests_count_on_last_request_time: usize,
        last_request_time: DateTime,
        period_duration: Duration,
    ) {
        let current_time = Utc::now();

        //self
    }
}
