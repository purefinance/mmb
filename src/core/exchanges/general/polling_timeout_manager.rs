use anyhow::Result;

use chrono::Utc;

use crate::core::{
    exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments,
    lifecycle::cancellation_token::CancellationToken, DateTime,
};

#[derive(Default)]
pub(crate) struct PollingTimeoutManager {
    timeout_arguments: RequestTimeoutArguments,
}

impl PollingTimeoutManager {
    pub(crate) fn new(timeout_arguments: RequestTimeoutArguments) -> Self {
        Self { timeout_arguments }
    }

    pub(crate) async fn wait(
        &self,
        last_request_date_time: Option<DateTime>,
        request_range: f32,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        let last_request_date_time = match last_request_date_time {
            Some(last_request_date_time) => last_request_date_time,
            None => return Ok(()),
        };

        let period = self.timeout_arguments.period;
        let requests_per_period = self.timeout_arguments.requests_per_period;

        // FIXME How to divide Duration by f32?
        let divisor = requests_per_period as f32 * request_range * 0.01;
        let interval = period / divisor as i32;

        let time_since_last_request = Utc::now() - last_request_date_time;
        let delay_till_fallback_request = interval - time_since_last_request;

        if delay_till_fallback_request.num_milliseconds() > 0 {
            let sleep = tokio::time::sleep(delay_till_fallback_request.to_std()?);
            tokio::select! {
                _ = sleep => {}
                _ = cancellation_token.when_cancelled() => {}
            }
        }

        Ok(())
    }
}
