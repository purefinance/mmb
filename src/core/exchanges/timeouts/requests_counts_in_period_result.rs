#[derive(Default)]
pub struct RequestsCountsInPeriodResult {
    pub(crate) requests_count: usize,
    pub(crate) reserved_in_groups_requests_count: usize,
    pub(crate) vacant_and_reserved_in_groups_requests_count: usize,
}

impl RequestsCountsInPeriodResult {
    pub fn new(
        requests_count: usize,
        reserved_in_groups_requests_count: usize,
        vacant_and_reserved_in_groups_requests_count: usize,
    ) -> Self {
        Self {
            requests_count,
            reserved_in_groups_requests_count,
            vacant_and_reserved_in_groups_requests_count,
        }
    }
}
