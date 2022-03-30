use std::collections::HashMap;

use super::{
    more_or_equals_available_requests_count_trigger_scheduler::MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    pre_reserved_group::PreReservedGroup, request::Request,
    triggers::handle_trigger_trait::TriggerHandler,
};
use crate::exchanges::timeouts::requests_timeout_manager::RequestGroupId;
use crate::{exchanges::common::ExchangeAccountId, exchanges::general::request_type::RequestType};
use anyhow::{anyhow, bail, Result};
use chrono::Duration;
use mmb_utils::DateTime;

pub(super) struct InnerRequestsTimeoutManager {
    pub(super) requests_per_period: usize,
    pub(super) period_duration: Duration,
    pub(super) exchange_account_id: ExchangeAccountId,
    pub(super) requests: Vec<Request>,
    pub(super) pre_reserved_groups: Vec<PreReservedGroup>,
    pub(super) last_time: Option<DateTime>,

    pub(super) group_was_reserved: Box<dyn Fn(PreReservedGroup) -> Result<()> + Send>,
    pub(super) group_was_removed: Box<dyn Fn(PreReservedGroup) -> Result<()> + Send>,
    pub(super) time_has_come_for_request: Box<dyn Fn(Request) -> Result<()> + Send>,

    pub(super) less_or_equals_requests_count_triggers: Vec<Box<dyn TriggerHandler + Send>>,
    pub(super) more_or_equals_available_requests_count_trigger_scheduler:
        MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    pub(super) delay_to_next_time_period: Duration,
    // data_recorder
}

impl InnerRequestsTimeoutManager {
    pub(super) fn try_reserve_request_instant(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
    ) -> Result<bool> {
        let current_time = self.get_non_decreasing_time(current_time);
        self.remove_outdated_requests(current_time)?;

        let _all_available_requests_count = self.get_all_available_requests_count();
        let available_requests_count = self.get_available_requests_count_at_present(current_time);

        if available_requests_count == 0 {
            // TODO save to DataRecorder

            return Ok(false);
        }

        let request = self.add_request(request_type, current_time, None)?;
        self.last_time = Some(current_time);

        log::info!(
            "Reserved request {:?} without group, instant {}",
            request_type,
            current_time
        );

        // TODO save to DataRecorder

        (self.time_has_come_for_request)(request)?;

        Ok(true)
    }

    pub(super) fn get_reserved_request_count_for_group_to_now(
        &self,
        group_id: RequestGroupId,
        current_time: DateTime,
    ) -> usize {
        let mut count = 0;

        let group_id = Some(group_id);
        for request in &self.requests {
            if request.allowed_start_time <= current_time && request.group_id == group_id {
                count += 1;
            }
        }

        count
    }

    pub(super) fn add_request(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
        group_id: Option<RequestGroupId>,
    ) -> Result<Request> {
        let request = Request::new(request_type, current_time, group_id);

        let request_index = self
            .requests
            .binary_search_by_key(&request.allowed_start_time, |stored_request| {
                stored_request.allowed_start_time
            })
            .map_or_else(|error_index| error_index, |ok_index| ok_index);

        self.requests.insert(request_index, request.clone());

        self.handle_all_decreasing_triggers()?;
        self.handle_all_increasing_triggers()?;

        Ok(request)
    }

    pub(super) fn handle_all_decreasing_triggers(&mut self) -> Result<()> {
        let available_requests_count = self.get_all_available_requests_count();

        for trigger in self.less_or_equals_requests_count_triggers.iter_mut() {
            trigger.handle(available_requests_count)?
        }

        Ok(())
    }

    pub(super) fn handle_all_increasing_triggers(&self) -> Result<()> {
        let available_requests_count_on_last_request_time =
            self.get_available_requests_in_last_period()?;

        self.more_or_equals_available_requests_count_trigger_scheduler
            .schedule_triggers(
                available_requests_count_on_last_request_time,
                self.get_last_request()?.allowed_start_time,
                self.period_duration,
            );

        Ok(())
    }

    pub(super) fn get_available_requests_in_last_period(&self) -> Result<usize> {
        let reserved_requests_count = self.get_requests_count_at_last_request_time()?;
        let reserved_requests_counts_without_group = reserved_requests_count
            .requests_count
            .saturating_sub(reserved_requests_count.reserved_in_groups_requests_count);
        let requests_difference = self.requests_per_period.saturating_sub(
            reserved_requests_counts_without_group
                + reserved_requests_count.vacant_and_reserved_in_groups_requests_count,
        );

        Ok(requests_difference)
    }

    fn get_requests_count_at_last_request_time(&self) -> Result<RequestsCountsInPeriodResult> {
        let last_request = self.get_last_request()?;
        let last_requests_start_time = last_request.allowed_start_time;
        let period_before_last = last_requests_start_time - self.period_duration;

        let not_period_predicate =
            |request: &Request, _| period_before_last > request.allowed_start_time;

        Ok(self.reserved_requests_count_in_period(period_before_last, not_period_predicate))
    }

    pub(super) fn get_last_request(&self) -> Result<Request> {
        self.requests.last().cloned().ok_or(anyhow!(
            "There are no stored request at all in TimeoutManager, so unable to get the last one"
        ))
    }

    pub(super) fn get_available_requests_count_at_present(&self, current_time: DateTime) -> usize {
        let reserved_requests_count = self.get_reserved_requests_count_at_present(current_time);
        let reserved_requests_counts_without_group = reserved_requests_count
            .requests_count
            .saturating_sub(reserved_requests_count.reserved_in_groups_requests_count);

        self.requests_per_period.saturating_sub(
            reserved_requests_counts_without_group
                + reserved_requests_count.vacant_and_reserved_in_groups_requests_count,
        )
    }

    fn get_reserved_requests_count_at_present(
        &self,
        current_time: DateTime,
    ) -> RequestsCountsInPeriodResult {
        let not_period_predicate = |request: &Request, time| request.allowed_start_time > time;
        self.reserved_requests_count_in_period(current_time, not_period_predicate)
    }

    fn reserved_requests_count_in_period<F>(
        &self,
        current_time: DateTime,
        not_period_predicate: F,
    ) -> RequestsCountsInPeriodResult
    where
        F: Fn(&Request, DateTime) -> bool,
    {
        let pre_reserved_groups = &self.pre_reserved_groups;
        let mut requests_count_by_group_id = HashMap::with_capacity(pre_reserved_groups.len());

        pre_reserved_groups.iter().for_each(|pre_reserved_group| {
            requests_count_by_group_id.insert(
                pre_reserved_group.id,
                RequestsCountTpm::new(pre_reserved_group.pre_reserved_requests_count),
            );
        });

        let mut requests_count = 0;
        let mut requests_count_in_group = 0;
        for request in self.requests.iter() {
            if not_period_predicate(request, current_time) {
                continue;
            }

            requests_count += 1;

            match request.group_id {
                None => continue,
                Some(group_id) => {
                    match requests_count_by_group_id.get_mut(&group_id) {
                        None => {
                            // if some requests from removed groups there are in current period we count its
                            // as requests out of group
                            continue;
                        }
                        Some(requests_count_tmp) => {
                            requests_count_in_group += 1;

                            requests_count_tmp.requests_count += 1;
                        }
                    }
                }
            }
        }
        let mut vacant_and_reserved_count = 0;
        for pair in requests_count_by_group_id {
            let requests_count_tmp = pair.1;
            if requests_count_tmp.requests_count <= requests_count_tmp.pre_reserved_count {
                vacant_and_reserved_count += requests_count_tmp.pre_reserved_count;
            } else {
                vacant_and_reserved_count += requests_count_tmp.requests_count;
            }
        }

        RequestsCountsInPeriodResult::new(
            requests_count,
            requests_count_in_group,
            vacant_and_reserved_count,
        )
    }

    pub(super) fn get_all_available_requests_count(&self) -> usize {
        self.requests_per_period.saturating_sub(self.requests.len())
    }

    pub(super) fn remove_outdated_requests(&mut self, current_time: DateTime) -> Result<()> {
        let deadline = current_time
            .checked_sub_signed(self.period_duration)
            .ok_or(anyhow!("Unable to subtract time periods"))?;
        self.requests
            .retain(|request| request.allowed_start_time >= deadline);

        Ok(())
    }

    pub(super) fn get_non_decreasing_time(&self, time: DateTime) -> DateTime {
        let last_time = self.last_time;

        match last_time {
            None => time,
            Some(time_value) => {
                if time_value < time {
                    time
                } else {
                    time_value
                }
            }
        }
    }

    pub(super) fn check_threshold(&self, count_threshold: usize) -> Result<()> {
        if self.requests_per_period < count_threshold {
            bail!("Unable to register trigger with count threshold more then available request for period. {} > {} for {}",
                count_threshold,
                self.requests_per_period,
                self.exchange_account_id)
        }

        Ok(())
    }
}

#[derive(Default)]
struct RequestsCountsInPeriodResult {
    requests_count: usize,
    reserved_in_groups_requests_count: usize,
    vacant_and_reserved_in_groups_requests_count: usize,
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

struct RequestsCountTpm {
    requests_count: usize,
    pre_reserved_count: usize,
}

impl RequestsCountTpm {
    fn new(pre_reserved_count: usize) -> Self {
        Self {
            requests_count: 0,
            pre_reserved_count,
        }
    }
}
