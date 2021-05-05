use crate::core::DateTime;
use anyhow::Result;
use chrono::{Duration, Utc};
use log::error;
use parking_lot::Mutex;
use tokio::time::sleep;

pub struct MoreOrEqualsAvailableRequestsCountTriggerScheduler {
    increasing_count_triggers: Mutex<Vec<MoreOrEqualsAvailableRequestsCountTrigger>>,
}

impl MoreOrEqualsAvailableRequestsCountTriggerScheduler {
    pub fn utc_now() -> DateTime {
        Utc::now()
    }

    pub fn register_trigger(&self, count_threshold: usize, handler: Box<dyn Fn() -> Result<()>>) {
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
    handler: Box<dyn Fn() -> Result<()>>,
}

impl MoreOrEqualsAvailableRequestsCountTrigger {
    fn new(count_threshold: usize, handler: Box<dyn Fn() -> Result<()>>) -> Self {
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
        let is_greater = available_requests_count_on_last_request_time >= self.count_threshold;

        if is_greater {
            return;
        }

        // Note: suppose that requests restriction same as in RequestsTimeoutManager (requests count in specified time period)
        // It logical dependency to RequestsTimeoutManager how calculate trigger time
        // var triggerTime = isGreater ? lastRequestTime : lastRequestTime + periodDuration;
        let trigger_time = last_request_time + period_duration;
        let mut delay = trigger_time - current_time;
        delay = if delay < Duration::zero() {
            Duration::zero()
        } else {
            delay
        };

        let _async_handler = self.handle_inner(delay);
        // FIXME How to run that future like in C#
        // Task.Run(() => task)
    }

    async fn handle_inner(&self, delay: Duration) {
        if let Ok(delay) = delay.to_std() {
            sleep(delay).await;
            if let Err(error) = (*self.handler)() {
                error!(
                    "Eror in MoreOrEqualsAvailableRequestsCountTrigger: {}",
                    error
                );
            }
        } else {
            error!("Unable to convert chrono::Duration to std::Duration");
        }
    }
}

#[cfg(test)]
mod test {
    use chrono::{NaiveDateTime, Utc};
    use DateTime;

    use super::*;

    #[test]
    fn negative_delay() {
        let handler = Box::new(|| Ok(()));
        let trigger = MoreOrEqualsAvailableRequestsCountTrigger::new(5, handler);
        let wrong_date_time = DateTime::from_utc(NaiveDateTime::from_timestamp(0, 0), Utc);
        trigger.schedule_handler(3, wrong_date_time, Duration::seconds(5), Utc::now());
    }
}
