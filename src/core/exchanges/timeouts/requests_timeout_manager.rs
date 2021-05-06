use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::time::sleep;

use anyhow::{anyhow, bail, Result};
use chrono::Duration;
use log::{error, info};
use uuid::Uuid;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::common::ExchangeAccountId,
    exchanges::general::request_type::RequestType, exchanges::utils, DateTime,
};

use super::{
    more_or_equals_available_requests_count_trigger_scheduler::MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    pre_reserved_group::PreReservedGroup, request::Request,
    requests_counts_in_period_result::RequestsCountsInPeriodResult,
    triggers::every_requests_count_change_trigger::EveryRequestsCountChangeTrigger,
    triggers::handle_trigger_trait::TriggerHandler,
    triggers::less_or_equals_requests_count_trigger::LessOrEqualsRequestsCountTrigger,
};

pub struct RequestsTimeoutManager {
    pub state: RwLock<InnerRequestsTimeoutManager>,
}

pub struct InnerRequestsTimeoutManager {
    requests_per_period: usize,
    period_duration: Duration,
    exchange_account_id: ExchangeAccountId,
    pub requests: Vec<Request>,
    pre_reserved_groups: Vec<PreReservedGroup>,
    last_time: Option<DateTime>,

    pub group_was_reserved: Option<Box<dyn Fn(PreReservedGroup) -> Result<()>>>,
    pub group_was_removed: Option<Box<dyn Fn(PreReservedGroup) -> Result<()>>>,
    pub time_has_come_for_request: Option<Box<dyn Fn(Request) -> Result<()>>>,

    less_or_equals_requests_count_triggers: Vec<Box<dyn TriggerHandler>>,
    more_or_equals_available_requests_count_trigger_scheduler:
        MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    delay_to_next_time_period: Duration,
    // data_recorder
}

impl RequestsTimeoutManager {
    pub fn new(
        requests_per_period: usize,
        period_duration: Duration,
        exchange_account_id: ExchangeAccountId,
        more_or_equals_available_requests_count_trigger_scheduler: MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    ) -> Self {
        let state = InnerRequestsTimeoutManager {
            requests_per_period,
            period_duration,
            exchange_account_id,
            requests: Default::default(),
            pre_reserved_groups: Default::default(),
            last_time: None,
            delay_to_next_time_period: Duration::milliseconds(1),
            group_was_reserved: None,
            group_was_removed: None,
            time_has_come_for_request: None,
            less_or_equals_requests_count_triggers: Default::default(),
            more_or_equals_available_requests_count_trigger_scheduler,
        };

        Self {
            state: RwLock::new(state),
        }
    }

    pub fn try_reserve_group(
        &mut self,
        group_type: String,
        current_time: DateTime,
        requests_count: usize,
        // call_source: SourceInfo, // TODO not needed until DataRecorder is ready
    ) -> Result<Option<Uuid>> {
        let mut state = self.state.write();

        let current_time = state.get_non_decreasing_time(current_time);
        state.remove_outdated_requests(current_time)?;

        let _all_available_requests_count = state.get_all_available_requests_count();
        let available_requests_count = state.get_available_requests_count_at_persent(current_time);

        if available_requests_count < requests_count {
            // TODO save to DataRecorder
            return Ok(None);
        }

        let group_id = Uuid::new_v4();
        let group = PreReservedGroup::new(group_id, group_type, requests_count);
        state.pre_reserved_groups.push(group.clone());

        info!("PreReserved grop {} {} was added", group_id, requests_count);

        // TODO save to DataRecorder

        state.last_time = Some(current_time);

        utils::try_invoke(&state.group_was_reserved, group)?;

        Ok(Some(group_id))
    }

    pub fn remove_group(&mut self, group_id: Uuid, _current_time: DateTime) -> Result<bool> {
        let mut state = self.state.write();

        let _all_available_requests_count = state.get_all_available_requests_count();
        let stored_group = state
            .pre_reserved_groups
            .iter()
            .position(|group| group.id == group_id);

        match stored_group {
            None => {
                error!("Cannot find PreReservedGroup {} for removing", { group_id });
                // TODO save to DataRecorder

                Ok(false)
            }
            Some(group_index) => {
                let group = state.pre_reserved_groups[group_index].clone();
                let pre_reserved_requests_count = group.pre_reserved_requests_count;
                state.pre_reserved_groups.remove(group_index);

                info!(
                    "PreReservedGroup {} {} was removed",
                    group_id, pre_reserved_requests_count
                );

                // TODO save to DataRecorder

                utils::try_invoke(&state.group_was_removed, group)?;

                Ok(true)
            }
        }
    }

    pub fn try_reserve_instant(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
        pre_reserved_group_id: Option<Uuid>,
    ) -> Result<bool> {
        match pre_reserved_group_id {
            Some(pre_reserved_group_id) => {
                self.try_reserve_group_instant(request_type, current_time, pre_reserved_group_id)
            }
            None => self.try_reserve_request_instant(request_type, current_time),
        }
    }

    pub fn try_reserve_group_instant(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
        pre_reserved_group_id: Uuid,
    ) -> Result<bool> {
        let mut state = self.state.write();
        let group = state
            .pre_reserved_groups
            .iter()
            .find(|group| group.id == pre_reserved_group_id);

        match group {
            None => {
                error!(
                    "Cannot find PreReservedGroup {} for reserve requests instant {:?}",
                    pre_reserved_group_id, request_type
                );

                // TODO save to DataRecorder

                return state.try_reserve_request_instant(request_type, current_time);
            }
            Some(group) => {
                let group = group.clone();
                let current_time = state.get_non_decreasing_time(current_time);
                state.remove_outdated_requests(current_time)?;

                let all_available_requests_count = state.get_all_available_requests_count();
                let available_requests_count_without_group =
                    state.get_available_requests_count_at_persent(current_time);
                let reserved_requests_count_for_group = state
                    .get_reserved_request_count_for_group_to_now(
                        pre_reserved_group_id,
                        current_time,
                    );

                let rest_requests_count_in_group = group
                    .pre_reserved_requests_count
                    .saturating_sub(reserved_requests_count_for_group);
                let available_requests_count =
                    available_requests_count_without_group + rest_requests_count_in_group;

                if available_requests_count == 0 {
                    // TODO save to DataRecorder

                    return Ok(false);
                }

                let request =
                    state.add_request(request_type.clone(), current_time, Some(group.id))?;

                info!(
                    "Request {:?} reserved for group {} {} {} {}, instant {}",
                    request_type,
                    pre_reserved_group_id,
                    all_available_requests_count,
                    state.pre_reserved_groups.len(),
                    available_requests_count_without_group,
                    current_time
                );

                utils::try_invoke(&state.time_has_come_for_request, request)?;

                Ok(true)
            }
        }
    }

    pub fn try_reserve_request_instant(
        &self,
        request_type: RequestType,
        current_time: DateTime,
    ) -> Result<bool> {
        self.state
            .write()
            .try_reserve_request_instant(request_type, current_time)
    }

    pub async fn reserve_when_available(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
        cancellation_token: CancellationToken,
    ) -> Result<(DateTime, Duration)> {
        // Note: calculation doesnt' support request cancellation
        // Note: suppose that exchange restriction work as your have n request on period and n request from beginning of next period and so on

        // Algorithm:
        // 1. We check: can we do request now
        // 2. if not form schedule for request where put at start period by requestsPerPeriod requests

        let mut state = self.state.write();

        let current_time = state.get_non_decreasing_time(current_time);
        state.remove_outdated_requests(current_time)?;

        let _available_requests_count = state.get_all_available_requests_count();

        // FIXME rewrite it easier
        let mut request_start_time;
        let delay;
        let available_requests_count_for_period;
        let request = {
            if state.requests.is_empty() {
                request_start_time = current_time;
                delay = Duration::zero();
                available_requests_count_for_period = state.requests_per_period;
                state.add_request(request_type.clone(), current_time, None)?
            } else {
                let last_request = state.get_last_request()?;
                let last_requests_start_time = last_request.allowed_start_time;

                available_requests_count_for_period =
                    state.get_available_requests_in_last_period()?;
                request_start_time = if available_requests_count_for_period == 0 {
                    last_requests_start_time
                        + state.period_duration
                        + state.delay_to_next_time_period
                } else {
                    last_requests_start_time
                };

                request_start_time = std::cmp::max(request_start_time, current_time);
                delay = request_start_time - current_time;
                state.add_request(request_type.clone(), request_start_time, None)?
            }
        };

        info!(
            "Request {:?} reserved, available {}",
            request_type, request_start_time
        );

        // TODO save to DataRecorder. Delete drop
        drop(available_requests_count_for_period);

        state.last_time = Some(current_time);

        drop(state);

        self.wait_for_request_availability(request, delay, cancellation_token)
            .await?;

        Ok((request_start_time, delay))
    }

    pub fn register_trigger_on_more_or_equals(
        &self,
        available_requests_count_threshold: usize,
        handler: Box<dyn Fn() -> Result<()>>,
    ) -> Result<()> {
        let state = self.state.read();
        state.check_threshold(available_requests_count_threshold)?;
        state
            .more_or_equals_available_requests_count_trigger_scheduler
            .register_trigger(available_requests_count_threshold, handler);

        Ok(())
    }

    pub fn register_trigger_on_less_or_equals(
        &self,
        available_requests_count_threshold: usize,
        handler: Box<dyn Fn() -> Result<()>>,
    ) -> Result<()> {
        let mut state = self.state.write();

        state.check_threshold(available_requests_count_threshold)?;
        let trigger =
            LessOrEqualsRequestsCountTrigger::new(available_requests_count_threshold, handler);
        state
            .less_or_equals_requests_count_triggers
            .push(Box::new(trigger));

        Ok(())
    }

    pub fn register_trigger_on_every_change(&self, handler: Box<dyn Fn(usize) -> Result<()>>) {
        let mut state = self.state.write();

        let trigger = EveryRequestsCountChangeTrigger::new(handler);
        state
            .less_or_equals_requests_count_triggers
            .push(Box::new(trigger));
    }

    async fn wait_for_request_availability(
        &self,
        request: Request,
        delay: Duration,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        match delay.to_std() {
            Ok(delay) => {
                let sleep_future = sleep(delay);
                let cancellation_token = cancellation_token.when_cancelled();

                tokio::select! {
                    _ = sleep_future => {
                        utils::try_invoke(&self.state.read().time_has_come_for_request, request)?;
                    }

                    _ = cancellation_token => {
                        utils::try_invoke(&self.state.read().time_has_come_for_request, request.clone())?;
                        self.state.write().requests.retain(|stored_request| *stored_request != request);

                        bail!("Operation cancelled")
                    }
                };

                Ok(())
            }
            Err(error) => {
                error!("Unable to convert chrono::Duration to std::Duration");
                bail!(error)
            }
        }
    }
}

impl InnerRequestsTimeoutManager {
    pub fn try_reserve_request_instant(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
    ) -> Result<bool> {
        let current_time = self.get_non_decreasing_time(current_time);
        self.remove_outdated_requests(current_time)?;

        let _all_available_requests_count = self.get_all_available_requests_count();
        let available_requests_count = self.get_available_requests_count_at_persent(current_time);

        if available_requests_count == 0 {
            // TODO save to DataRecorder

            return Ok(false);
        }

        let request = self.add_request(request_type.clone(), current_time, None)?;
        self.last_time = Some(current_time);

        info!(
            "Reserved request {:?} without group, instant {:?}",
            request_type, current_time
        );

        // TODO save to DataRecorder

        utils::try_invoke(&self.time_has_come_for_request, request)?;

        Ok(true)
    }

    fn get_reserved_request_count_for_group_to_now(
        &self,
        group_id: Uuid,
        current_time: DateTime,
    ) -> usize {
        let mut count = 0;

        for request in &self.requests {
            if let Some(request_group_id) = request.group_id {
                if request.allowed_start_time <= current_time && request_group_id == group_id {
                    count += 1;
                }
            }
        }

        count
    }

    fn add_request(
        &mut self,
        request_type: RequestType,
        current_time: DateTime,
        group_id: Option<Uuid>,
    ) -> Result<Request> {
        let request = Request::new(request_type, current_time, group_id);

        let request_index = self.requests.binary_search_by(|stored_request| {
            stored_request
                .allowed_start_time
                .cmp(&request.allowed_start_time)
        });

        let request_index =
            request_index.map_or_else(|error_index| error_index, |ok_index| ok_index);

        self.requests.insert(request_index, request.clone());

        self.handle_all_decreasing_triggers()?;
        self.handle_all_increasing_triggers()?;

        Ok(request)
    }

    fn handle_all_decreasing_triggers(&mut self) -> Result<()> {
        let available_requests_count = self.get_all_available_requests_count();
        let mut maybe_error = Ok(());
        self.less_or_equals_requests_count_triggers
            .iter_mut()
            .for_each(|trigger| {
                maybe_error = trigger.handle(available_requests_count);
            });

        maybe_error?;

        Ok(())
    }

    fn handle_all_increasing_triggers(&self) -> Result<()> {
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

    fn get_available_requests_in_last_period(&self) -> Result<usize> {
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

    fn get_last_request(&self) -> Result<Request> {
        self.requests
            .last()
            .map(|request| request.clone())
            .ok_or(anyhow!("There are no last request"))
    }

    fn get_available_requests_count_at_persent(&self, current_time: DateTime) -> usize {
        let reserved_requests_count = self.get_reserved_requests_count_at_present(current_time);
        let reserved_requests_counts_without_group = reserved_requests_count
            .requests_count
            .saturating_sub(reserved_requests_count.reserved_in_groups_requests_count);
        let available_requests_count = self.requests_per_period.saturating_sub(
            reserved_requests_counts_without_group
                + reserved_requests_count.vacant_and_reserved_in_groups_requests_count,
        );

        available_requests_count
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

    fn get_all_available_requests_count(&self) -> usize {
        let available_requests_number =
            self.requests_per_period.saturating_sub(self.requests.len());

        available_requests_number
    }

    fn remove_outdated_requests(&mut self, current_time: DateTime) -> Result<()> {
        let deadline = current_time
            .checked_sub_signed(self.period_duration)
            .ok_or(anyhow!("Unable to substract time periods"))?;
        self.requests
            .retain(|request| request.allowed_start_time >= deadline);

        Ok(())
    }

    fn get_non_decreasing_time(&self, time: DateTime) -> DateTime {
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

    fn check_threshold(&self, count_threshold: usize) -> Result<()> {
        if self.requests_per_period < count_threshold {
            bail!("Unable to register trigger with count threshold more then available request for period. {} > {} for {}",
                count_threshold,
                self.requests_per_period,
                self.exchange_account_id)
        }

        Ok(())
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

#[cfg(test)]
mod test {
    use chrono::Utc;

    use super::*;

    #[test]
    fn try_reserve_group_instant_test() {
        let mut manager = RequestsTimeoutManager::new(
            5,
            Duration::seconds(1),
            ExchangeAccountId::new("test".into(), 0),
            MoreOrEqualsAvailableRequestsCountTriggerScheduler::new(),
        );

        let result =
            manager.try_reserve_group_instant(RequestType::CreateOrder, Utc::now(), Uuid::new_v4());
        dbg!(&result);
    }
}
