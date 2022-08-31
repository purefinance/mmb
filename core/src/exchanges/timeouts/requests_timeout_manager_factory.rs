use std::{
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use chrono::{Duration, Utc};
use mmb_utils::DateTime;

use domain::market::ExchangeAccountId;

use super::{
    more_or_equals_available_requests_count_trigger_scheduler::MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    requests_timeout_manager::RequestsTimeoutManager,
};

pub struct RequestsTimeoutManagerFactory {}

impl RequestsTimeoutManagerFactory {
    pub fn utc_now() -> DateTime {
        Utc::now()
    }

    pub fn from_requests_per_period(
        timeout_arguments: RequestTimeoutArguments,
        exchange_account_id: ExchangeAccountId,
    ) -> Arc<RequestsTimeoutManager> {
        let trigger_scheduler = MoreOrEqualsAvailableRequestsCountTriggerScheduler::default();
        RequestsTimeoutManager::new(
            timeout_arguments.requests_per_period,
            timeout_arguments.period,
            exchange_account_id,
            trigger_scheduler,
        )
    }
}

pub struct RequestTimeoutArguments {
    pub requests_per_period: usize,
    pub period: Duration,
}

impl RequestTimeoutArguments {
    pub(crate) fn new(requests_per_period: usize, period: Duration) -> Self {
        Self {
            requests_per_period,
            period,
        }
    }

    pub fn unlimited() -> RequestTimeoutArguments {
        Self::from_requests_per_second(usize::MAX)
    }

    pub fn from_requests_per_second(requests_per_period: usize) -> RequestTimeoutArguments {
        let period = Duration::seconds(1);
        Self::new(requests_per_period, period)
    }

    pub fn from_requests_per_minute(requests_per_period: usize) -> RequestTimeoutArguments {
        let period = Duration::minutes(1);
        Self::new(requests_per_period, period)
    }

    pub fn from_requests_per_five_minute(requests_per_period: usize) -> RequestTimeoutArguments {
        let period = Duration::minutes(5);
        Self::new(requests_per_period, period)
    }

    pub fn from_requests_per_hour(requests_per_period: usize) -> RequestTimeoutArguments {
        let period = Duration::hours(1);
        Self::new(requests_per_period, period)
    }
}

impl Display for RequestTimeoutArguments {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Requests per period: {}, period: {}",
            self.requests_per_period, self.period
        )
    }
}
