use chrono::{Duration, Utc};
use parking_lot::Mutex;

use crate::core::DateTime;

pub struct MoreOrEqualsAvailableRequestsCountTriggerScheduler {
    increasing_count_triggers: Mutex<Vec<MoreOrEqualsAvailableRequestsCountTrigger>>,
}

impl MoreOrEqualsAvailableRequestsCountTriggerScheduler {
    pub fn utc_now() -> DateTime {
        Utc::now()
    }

    pub fn register_trigger(&self, count_threshold: usize, handler: Box<dyn Fn()>) {
        let trigger = MoreOrEqualsAvailableRequestsCountTrigger::new(count_threshold, handler);
        self.increasing_count_triggers.lock().push(trigger);
    }

    pub fn schedule_triggers(
        &self,
        available_requests_count_on_last_request_time: usize,
        last_request_time: DateTime,
        period_duration: Duration,
    ) {
        let current_time = Self::utc_now();

        for trigger in self.increasing_count_triggers.lock().iter() {
            trigger.schedule_handler(
                available_requests_count_on_last_request_time,
                last_request_time,
                period_duration,
                current_time,
            );
        }
    }
}

struct MoreOrEqualsAvailableRequestsCountTrigger {
    count_threshold: usize,
    handler: Box<dyn Fn()>,
}

impl MoreOrEqualsAvailableRequestsCountTrigger {
    fn new(count_threshold: usize, handler: Box<dyn Fn()>) -> Self {
        Self {
            count_threshold,
            handler,
        }
    }

    pub fn schedule_handler(
        &self,
        available_requests_count_on_last_request_time: usize,
        last_request_time: DateTime,
        period_duration: Duration,
        current_time: DateTime,
    ) {
    }
}
