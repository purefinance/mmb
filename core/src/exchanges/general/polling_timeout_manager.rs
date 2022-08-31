use chrono::{Duration, Utc};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::DateTime;
use tokio::time::timeout;

use crate::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_utils::time::ToStdExpected;

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
            #[allow(clippy::single_match)]
            match timeout(
                delay_till_fallback_request.to_std_expected(),
                cancellation_token.when_cancelled(),
            )
            .await
            {
                Ok(_) => {}
                Err(_) => {}
            };
        }
    }
}
