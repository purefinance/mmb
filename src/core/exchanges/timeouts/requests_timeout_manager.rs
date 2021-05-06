use parking_lot::RwLock;
use tokio::time::sleep;

use anyhow::{bail, Result};
use chrono::Duration;
use log::{error, info};
use uuid::Uuid;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::common::ExchangeAccountId,
    exchanges::general::request_type::RequestType, exchanges::utils, DateTime,
};

use super::{
    inner_request_manager::InnerRequestsTimeoutManager,
    more_or_equals_available_requests_count_trigger_scheduler::MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    pre_reserved_group::PreReservedGroup, request::Request,
    triggers::every_requests_count_change_trigger::EveryRequestsCountChangeTrigger,
    triggers::less_or_equals_requests_count_trigger::LessOrEqualsRequestsCountTrigger,
};

pub struct RequestsTimeoutManager {
    pub state: RwLock<InnerRequestsTimeoutManager>,
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

#[cfg(test)]
mod test {
    use crate::core::exchanges::timeouts::requests_timeout_manager_factory::{
        RequestTimeoutArguments, RequestsTimeoutManagerFactory,
    };
    use chrono::Utc;

    use super::*;
    use rstest::{fixture, rstest};

    #[fixture]
    fn timeout_manager() -> RequestsTimeoutManager {
        let requests_per_period = 5;
        let exchange_account_id = ExchangeAccountId::new("test_exchange_account_id".into(), 0);
        let timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
            RequestTimeoutArguments::from_requests_per_minute(requests_per_period),
            exchange_account_id,
        );

        timeout_manager
    }

    mod try_reserve_group {
        use super::*;

        #[rstest]
        fn when_can_reserve(mut timeout_manager: RequestsTimeoutManager) -> Result<()> {
            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();

            // Act
            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;

            // Assert
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());

            assert_eq!(timeout_manager.state.read().pre_reserved_groups.len(), 2);
            let state = timeout_manager.state.read();
            let first_group = state.pre_reserved_groups.first().expect("in test");

            assert_eq!(first_group.id, first_group_id.expect("in test"));
            assert_eq!(first_group.group_type, group_type);
            assert_eq!(first_group.pre_reserved_requests_count, 3);

            let second_group = state.pre_reserved_groups[1].clone();

            assert_eq!(second_group.id, second_group_id.expect("in test"));
            assert_eq!(second_group.group_type, group_type);
            assert_eq!(second_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        fn not_enought_requests(mut timeout_manager: RequestsTimeoutManager) -> Result<()> {
            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;

            // Act
            let third_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 1)?;

            // Assert
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());
            assert!(third_group_id.is_none());

            assert_eq!(timeout_manager.state.read().pre_reserved_groups.len(), 2);
            let state = timeout_manager.state.read();
            let first_group = state.pre_reserved_groups.first().expect("in test");

            assert_eq!(first_group.id, first_group_id.expect("in test"));
            assert_eq!(first_group.group_type, group_type);
            assert_eq!(first_group.pre_reserved_requests_count, 3);

            let second_group = state.pre_reserved_groups[1].clone();

            assert_eq!(second_group.id, second_group_id.expect("in test"));
            assert_eq!(second_group.group_type, group_type);
            assert_eq!(second_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        fn when_reserve_after_remove(mut timeout_manager: RequestsTimeoutManager) -> Result<()> {
            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();
            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;

            let remove_result =
                timeout_manager.remove_group(second_group_id.expect("in test"), current_time)?;
            assert!(remove_result);

            // Act
            let third_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;

            // Assert
            assert!(third_group_id.is_none());

            assert_eq!(timeout_manager.state.read().pre_reserved_groups.len(), 1);
            let state = timeout_manager.state.read();
            let group = state.pre_reserved_groups.first().expect("in test");

            assert_eq!(group.id, first_group_id.expect("in test"));
            assert_eq!(group.group_type, group_type);
            assert_eq!(group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_requests_reserved_and_not_enough_requests(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();
            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;

            // Act
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;

            // Assert
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_none());

            assert_eq!(timeout_manager.state.read().requests.len(), 1);

            let state = timeout_manager.state.read();
            let group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(group.id, first_group_id.expect("in test"));
            assert_eq!(group.group_type, group_type);
            assert_eq!(group.pre_reserved_requests_count, 3);

            Ok(())
        }
    }
}
