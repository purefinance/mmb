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

    mod remove_group {
        use super::*;

        #[rstest]
        fn group_exists(mut timeout_manager: RequestsTimeoutManager) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;

            // Act
            timeout_manager.remove_group(second_group_id.expect("in test"), current_time)?;

            // Assert
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());

            assert_eq!(timeout_manager.state.read().pre_reserved_groups.len(), 1);

            let state = timeout_manager.state.read();
            let group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(group.id, first_group_id.expect("in test"));
            assert_eq!(group.group_type, group_type);
            assert_eq!(group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn group_not_exists(mut timeout_manager: RequestsTimeoutManager) -> Result<()> {
            // Arrange
            let current_time = Utc::now();

            // Act
            let removing_result = timeout_manager.remove_group(Uuid::new_v4(), current_time)?;

            // Assert
            assert!(!removing_result);

            Ok(())
        }
    }

    mod try_reserve_instant {
        use super::*;

        #[rstest]
        fn there_are_spare_requests_true(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let current_time = Utc::now();

            // Act
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let second_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(first_reserved);
            assert!(second_reserved);

            let state = timeout_manager.state.read();

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert!(first_request.group_id.is_none());

            let second_request = state.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[ignore] //< FIXME It's working too long
        #[rstest]
        #[tokio::test]
        async fn there_are_requests_from_future_false(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            timeout_manager.state.write().requests_per_period = 2;
            let current_time = Utc::now();
            let before_now = current_time - Duration::seconds(59);

            timeout_manager
                .reserve_when_available(
                    RequestType::CreateOrder,
                    before_now,
                    CancellationToken::default(),
                )
                .await?;
            timeout_manager
                .reserve_when_available(
                    RequestType::CreateOrder,
                    before_now,
                    CancellationToken::default(),
                )
                .await?;
            timeout_manager
                .reserve_when_available(
                    RequestType::CreateOrder,
                    before_now,
                    CancellationToken::default(),
                )
                .await?;

            // Act
            let reserve_result = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(!reserve_result);

            let state = timeout_manager.state.read();
            assert_eq!(state.requests.len(), 3);

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time - Duration::seconds(59)
            );
            assert!(first_request.group_id.is_none());

            let second_request = state.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time - Duration::seconds(59)
            );
            assert!(second_request.group_id.is_none());

            let third_request = state.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                third_request.allowed_start_time,
                current_time + Duration::seconds(1) + Duration::milliseconds(1)
            );
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[rstest]
        fn there_are_no_spare_requests_false(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            timeout_manager.state.write().requests_per_period = 2;
            let current_time = Utc::now();

            // Act
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let second_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let third_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(first_reserved);
            assert!(second_reserved);
            assert!(!third_reserved);

            let state = timeout_manager.state.read();

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert!(first_request.group_id.is_none());

            let second_request = state.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[rstest]
        fn outdated_request_get_removed(mut timeout_manager: RequestsTimeoutManager) -> Result<()> {
            // Arrange
            timeout_manager.state.write().requests_per_period = 2;
            let current_time = Utc::now();
            let before_now = current_time - Duration::seconds(71);

            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                before_now - Duration::seconds(1),
                None,
            )?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, before_now, None)?;

            let state = timeout_manager.state.read();

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time - Duration::seconds(72)
            );
            assert!(first_request.group_id.is_none());

            let second_request = state.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time - Duration::seconds(71)
            );
            assert!(second_request.group_id.is_none());

            drop(state);

            // Act
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let second_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let third_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(first_reserved);
            assert!(second_reserved);
            assert!(!third_reserved);

            let state = timeout_manager.state.read();

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert!(first_request.group_id.is_none());

            let second_request = state.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[rstest]
        fn when_remove_group_which_blocked_last_request(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            let reserve_result = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(!reserve_result);

            let remove_result =
                timeout_manager.remove_group(second_group_id.expect("in test"), current_time)?;
            assert!(remove_result);

            // Act
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let second_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let third_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(first_reserved);
            assert!(second_reserved);
            assert!(!third_reserved);

            let state = timeout_manager.state.read();

            let requests_len = state.requests.len();
            assert_eq!(requests_len, 2);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_cant_reserve_request_in_common_queue_because_of_group_reservations(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            assert!(first_group_id.is_some());

            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            assert_eq!(timeout_manager.state.read().requests.len(), 2);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(!reserved_instant);

            let state = timeout_manager.state.read();

            let requests_len = state.requests.len();
            assert_eq!(requests_len, 2);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_cant_reserve_request_without_group(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            assert!(first_group_id.is_some());

            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(!reserved_instant);
            assert_eq!(timeout_manager.state.read().requests.len(), 2);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(reserved_instant);

            let state = timeout_manager.state.read();

            let requests_len = state.requests.len();
            assert_eq!(requests_len, 3);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_cant_reserve_request_in_group(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            assert!(first_group_id.is_some());

            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            let third_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(!third_reserve_attempt);
            let fourth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(fourth_reserve_attempt);
            let fifth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(fifth_reserve_attempt);
            let sixth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(sixth_reserve_attempt);

            assert_eq!(timeout_manager.state.read().requests.len(), 5);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(!reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 5);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_recapturing_vacant_request_in_group_after_exhaustion(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            assert!(first_group_id.is_some());

            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            let third_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(!third_reserve_attempt);
            let fourth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(fourth_reserve_attempt);
            let fifth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(fifth_reserve_attempt);
            let sixth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(sixth_reserve_attempt);

            assert_eq!(timeout_manager.state.read().requests.len(), 5);

            let delay_to_next_time_period = Duration::milliseconds(1);
            let period_duration = timeout_manager.state.read().period_duration;

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time + period_duration + delay_to_next_time_period,
                first_group_id,
            )?;

            // Assert
            assert!(reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 1);
            assert_eq!(state.pre_reserved_groups.len(), 1);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_trying_reserve_without_group(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let current_time = Utc::now();

            // TODO test logger event here

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                Some(Uuid::new_v4()),
            )?;

            // Assert
            assert!(reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 1);
            assert!(state.pre_reserved_groups.is_empty());

            Ok(())
        }

        #[rstest]
        fn when_another_group_occupied_last_requests(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.state.write().requests_per_period = 4;
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());

            let first_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(first_reserve_attempt);
            let second_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(second_reserve_attempt);

            assert_eq!(timeout_manager.state.read().requests.len(), 2);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(!reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 2);

            assert_eq!(state.pre_reserved_groups.len(), 2);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            let second_reserved_group = state.pre_reserved_groups[1].clone();
            assert_eq!(second_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(second_reserved_group.group_type, group_type);
            assert_eq!(second_reserved_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[ignore] //< FIXME too big delay. Find out how to fix/mock it
        #[rstest]
        #[tokio::test]
        async fn when_there_is_request_in_the_future_time(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.state.write().requests_per_period = 4;
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            assert!(first_group_id.is_some());

            let first_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(first_reserve_attempt);
            let second_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(second_reserve_attempt);

            timeout_manager
                .reserve_when_available(
                    RequestType::CancelOrder,
                    current_time,
                    CancellationToken::default(),
                )
                .await?;
            let first_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(first_reserve_attempt);

            assert_eq!(timeout_manager.state.read().requests.len(), 4);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 5);

            assert_eq!(state.pre_reserved_groups.len(), 1);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        fn when_group_has_more_requests_then_preffered_count(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.state.write().requests_per_period = 4;
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 1)?;
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());

            let first_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(first_reserve_attempt);
            let second_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(second_reserve_attempt);
            let third_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(third_reserve_attempt);
            let fourth_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                second_group_id,
            )?;
            assert!(fourth_reserve_attempt);

            assert_eq!(timeout_manager.state.read().requests.len(), 4);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                second_group_id,
            )?;

            // Assert
            assert!(!reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 4);

            assert_eq!(state.pre_reserved_groups.len(), 2);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            let second_reserved_group = state.pre_reserved_groups[1].clone();
            assert_eq!(second_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(second_reserved_group.group_type, group_type);
            assert_eq!(second_reserved_group.pre_reserved_requests_count, 1);

            Ok(())
        }
    }

    mod reserve_when_available {
        use super::*;

        #[rstest]
        #[tokio::test]
        async fn no_current_requests_true(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            timeout_manager.state.write().requests_per_period = 1;
            let current_time = Utc::now();

            // Act
            let (available_start_time, delay) = timeout_manager
                .reserve_when_available(
                    RequestType::CreateOrder,
                    current_time,
                    CancellationToken::default(),
                )
                .await?;

            // Assert
            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 1);

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[rstest]
        #[tokio::test]
        async fn only_outdated_requests_true(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            timeout_manager.state.write().requests_per_period = 1;
            let current_time = Utc::now();

            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time - Duration::seconds(61),
                None,
            )?;

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 1);

            let request = state.requests.first().expect("in test");
            assert_eq!(request.request_type, RequestType::CreateOrder);
            assert_eq!(
                request.allowed_start_time,
                current_time - Duration::seconds(61)
            );
            assert_eq!(request.group_id, None);
            drop(state);

            // Act
            let (available_start_time, delay) = timeout_manager
                .reserve_when_available(
                    RequestType::CreateOrder,
                    current_time,
                    CancellationToken::default(),
                )
                .await?;

            // Assert
            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 1);

            let first_request = state.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            // TODO Probably should_be_close_to
            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[ignore] //< FIXME too big delay. Find out how to fix/mock it
        #[rstest]
        #[tokio::test]
        async fn when_all_requests_pre_reserved_in_groups(
            mut timeout_manager: RequestsTimeoutManager,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.state.write().requests_per_period = 4;
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 1)?;
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());

            // Act
            let first_reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            timeout_manager
                .reserve_when_available(
                    RequestType::CreateOrder,
                    current_time,
                    CancellationToken::default(),
                )
                .await?;
            let second_reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // NOTE: unexpected behaviour (group where we can reserve only 1 request)
            let third_reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(!first_reserved_instant);
            assert!(second_reserved_instant);
            assert!(!third_reserved_instant);

            let state = timeout_manager.state.read();

            assert_eq!(state.requests.len(), 2);

            assert_eq!(state.pre_reserved_groups.len(), 2);

            let first_reserved_group = state.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            let second_reserved_group = state.pre_reserved_groups[1].clone();
            assert_eq!(second_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(second_reserved_group.group_type, group_type);
            assert_eq!(second_reserved_group.pre_reserved_requests_count, 1);

            Ok(())
        }
    }
}
