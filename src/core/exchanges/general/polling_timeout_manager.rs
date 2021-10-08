use chrono::{Duration, Utc};

use crate::core::{
    exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments,
    lifecycle::cancellation_token::CancellationToken, DateTime,
};

pub(crate) struct PollingTimeoutManager {
    timeout_arguments: RequestTimeoutArguments,
}

impl PollingTimeoutManager {
    pub(crate) fn new(timeout_arguments: RequestTimeoutArguments) -> Self {
        Self { timeout_arguments }
    }

    pub(crate) async fn wait(
        &self,
        last_request_time: Option<DateTime>,
        request_range: f64,
        cancellation_token: CancellationToken,
    ) {
        let last_request_time = match last_request_time {
            Some(last_request_time) => last_request_time,
            None => return,
        };

        let period = self.timeout_arguments.period;
        let requests_per_period = self.timeout_arguments.requests_per_period;

        let divisor = requests_per_period as f64 * request_range * 0.01;
        let interval = Duration::milliseconds((period.num_milliseconds() as f64 / divisor) as i64);

        let time_since_last_request = Utc::now() - last_request_time;
        let delay_till_fallback_request = interval - time_since_last_request;

        if delay_till_fallback_request.num_milliseconds() > 0 {
            let sleep = tokio::time::sleep(
                delay_till_fallback_request
                    .to_std()
                    .expect("Unable to convert chrono::Duration to std::time::Duration in PollingTimeoutManager::wait()"),
            );
            tokio::select! {
                _ = sleep => {}
                _ = cancellation_token.when_cancelled() => {}
            }
        }
    }
}
