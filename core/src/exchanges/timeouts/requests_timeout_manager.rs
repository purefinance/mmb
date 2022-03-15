use std::fmt::{Display, Formatter};
use std::sync::{Arc, Weak};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Duration;
use futures::FutureExt;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::{FutureOutcome, SpawnFutureFlags};
use mmb_utils::{DateTime, OPERATION_CANCELED_MSG};
use parking_lot::Mutex;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use uuid::Uuid;

use super::{
    inner_request_manager::InnerRequestsTimeoutManager,
    more_or_equals_available_requests_count_trigger_scheduler::MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    pre_reserved_group::PreReservedGroup, request::Request,
    triggers::every_requests_count_change_trigger::EveryRequestsCountChangeTrigger,
    triggers::less_or_equals_requests_count_trigger::LessOrEqualsRequestsCountTrigger,
};
use crate::exchanges::common::ToStdExpected;
use crate::{
    exchanges::common::ExchangeAccountId, exchanges::general::request_type::RequestType,
    infrastructure::spawn_future,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct RequestGroupId(Uuid);

impl RequestGroupId {
    pub fn generate() -> Self {
        RequestGroupId(Uuid::new_v4())
    }
}

impl Display for RequestGroupId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct RequestsTimeoutManager {
    inner: Mutex<InnerRequestsTimeoutManager>,
}

impl RequestsTimeoutManager {
    pub fn new(
        requests_per_period: usize,
        period_duration: Duration,
        exchange_account_id: ExchangeAccountId,
        more_or_equals_available_requests_count_trigger_scheduler: MoreOrEqualsAvailableRequestsCountTriggerScheduler,
    ) -> Arc<Self> {
        let inner = InnerRequestsTimeoutManager {
            requests_per_period,
            period_duration,
            exchange_account_id,
            requests: Default::default(),
            pre_reserved_groups: Default::default(),
            last_time: None,
            delay_to_next_time_period: Duration::milliseconds(1),
            group_was_reserved: Box::new(|_| Ok(())),
            group_was_removed: Box::new(|_| Ok(())),
            time_has_come_for_request: Box::new(|_| Ok(())),
            less_or_equals_requests_count_triggers: Default::default(),
            more_or_equals_available_requests_count_trigger_scheduler,
        };

        Arc::new(Self {
            inner: Mutex::new(inner),
        })
    }

    pub fn try_reserve_group(
        &self,
        group_type: String,
        current_time: DateTime,
        requests_count: usize,
        // call_source: SourceInfo, // TODO not needed until DataRecorder is ready
    ) -> Result<Option<RequestGroupId>> {
        let mut inner = self.inner.lock();

        let current_time = inner.get_non_decreasing_time(current_time);
        inner.remove_outdated_requests(current_time)?;

        let _all_available_requests_count = inner.get_all_available_requests_count();
        let available_requests_count = inner.get_available_requests_count_at_present(current_time);

        if available_requests_count < requests_count {
            // TODO save to DataRecorder
            return Ok(None);
        }

        let group_id = RequestGroupId::generate();
        let group = PreReservedGroup::new(group_id, group_type, requests_count);
        inner.pre_reserved_groups.push(group.clone());

        log::info!(
            "PreReserved group with group_id {} and request_count {} was added",
            group_id,
            requests_count
        );

        // TODO save to DataRecorder

        inner.last_time = Some(current_time);

        (inner.group_was_reserved)(group)?;

        Ok(Some(group_id))
    }

    pub fn remove_group(&self, group_id: RequestGroupId, _current_time: DateTime) -> Result<bool> {
        let mut inner = self.inner.lock();

        let _all_available_requests_count = inner.get_all_available_requests_count();
        let stored_group = inner
            .pre_reserved_groups
            .iter()
            .position(|group| group.id == group_id);

        match stored_group {
            None => {
                log::error!("Cannot find PreReservedGroup {} for removing", { group_id });
                // TODO save to DataRecorder

                Ok(false)
            }
            Some(group_index) => {
                let group = inner.pre_reserved_groups[group_index].clone();
                let pre_reserved_requests_count = group.pre_reserved_requests_count;
                inner.pre_reserved_groups.remove(group_index);

                log::info!(
                    "PreReservedGroup with group_id {} and pre_reserved_requests_count {} was removed",
                    group_id, pre_reserved_requests_count
                );

                // TODO save to DataRecorder

                (inner.group_was_removed)(group)?;

                Ok(true)
            }
        }
    }

    pub fn try_reserve_instant(
        &self,
        request_type: RequestType,
        current_time: DateTime,
        pre_reserved_group_id: Option<RequestGroupId>,
    ) -> Result<bool> {
        match pre_reserved_group_id {
            Some(pre_reserved_group_id) => {
                self.try_reserve_group_instant(request_type, current_time, pre_reserved_group_id)
            }
            None => self.try_reserve_request_instant(request_type, current_time),
        }
    }

    pub fn try_reserve_group_instant(
        &self,
        request_type: RequestType,
        current_time: DateTime,
        pre_reserved_group_id: RequestGroupId,
    ) -> Result<bool> {
        let mut inner = self.inner.lock();
        let group = inner
            .pre_reserved_groups
            .iter()
            .find(|group| group.id == pre_reserved_group_id);

        match group {
            None => {
                log::error!(
                    "Cannot find PreReservedGroup {} for reserve requests instant {:?}",
                    pre_reserved_group_id,
                    request_type
                );

                // TODO save to DataRecorder

                inner.try_reserve_request_instant(request_type, current_time)
            }
            Some(group) => {
                let group = group.clone();
                let current_time = inner.get_non_decreasing_time(current_time);
                inner.remove_outdated_requests(current_time)?;

                let all_available_requests_count = inner.get_all_available_requests_count();
                let available_requests_count_without_group =
                    inner.get_available_requests_count_at_present(current_time);
                let reserved_requests_count_for_group = inner
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

                let request = inner.add_request(request_type, current_time, Some(group.id))?;

                log::info!(
                    "Request {:?} reserved for group with pre_reserved_group_id {},
                    all_available_requests_count {},
                    pre_reserved_groups.len() {},
                    available_requests_count_without_group {},
                    in time {}",
                    request_type,
                    pre_reserved_group_id,
                    all_available_requests_count,
                    inner.pre_reserved_groups.len(),
                    available_requests_count_without_group,
                    current_time
                );

                (inner.time_has_come_for_request)(request)?;

                Ok(true)
            }
        }
    }

    pub fn try_reserve_request_instant(
        &self,
        request_type: RequestType,
        current_time: DateTime,
    ) -> Result<bool> {
        self.inner
            .lock()
            .try_reserve_request_instant(request_type, current_time)
    }

    pub fn reserve_when_available(
        self: Arc<Self>,
        request_type: RequestType,
        current_time: DateTime,
        cancellation_token: CancellationToken,
    ) -> Result<(JoinHandle<FutureOutcome>, DateTime, Duration)> {
        // Note: calculation doesn't support request cancellation
        // Note: suppose that exchange restriction work as your have n request on period and n request from beginning of next period and so on

        // Algorithm:
        // 1. We check: can we do request now
        // 2. if not form schedule for request where put at start period by requestsPerPeriod requests

        let mut inner = self.inner.lock();

        let current_time = inner.get_non_decreasing_time(current_time);
        inner.remove_outdated_requests(current_time)?;

        let _available_requests_count = inner.get_all_available_requests_count();

        let mut request_start_time;
        let delay;
        let available_requests_count_for_period;
        let request = if inner.requests.is_empty() {
            request_start_time = current_time;
            delay = Duration::zero();
            // available_requests_count_for_period = inner.requests_per_period;
            inner.add_request(request_type, current_time, None)?
        } else {
            let last_request = inner.get_last_request()?;
            let last_requests_start_time = last_request.allowed_start_time;

            available_requests_count_for_period = inner.get_available_requests_in_last_period()?;
            request_start_time = if available_requests_count_for_period == 0 {
                last_requests_start_time + inner.period_duration + inner.delay_to_next_time_period
            } else {
                last_requests_start_time
            };

            request_start_time = std::cmp::max(request_start_time, current_time);
            delay = request_start_time - current_time;
            inner.add_request(request_type, request_start_time, None)?
        };

        log::info!(
            "Request {:?} reserved, available in request_start_time {}",
            request_type,
            request_start_time
        );

        // TODO save to DataRecorder. Delete drop
        // drop(available_requests_count_for_period);

        inner.last_time = Some(current_time);

        drop(inner);

        let action = Self::wait_for_request_availability(
            Arc::downgrade(&self),
            request,
            delay,
            cancellation_token,
        );
        let request_availability = spawn_future(
            "Waiting request in reserve_when_available()",
            SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::CRITICAL,
            action.boxed(),
        );

        Ok((request_availability, request_start_time, delay))
    }

    async fn wait_for_request_availability(
        weak_self: Weak<Self>,
        request: Request,
        delay: Duration,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        // Should never panic, because function wait_for_request_availability
        // has one call with guaranteed non-negative delay.
        let delay: std::time::Duration = delay.to_std_expected();

        let sleep_future = sleep(delay);
        let cancellation_token = cancellation_token.when_cancelled();

        tokio::select! {
            _ = sleep_future => {
                let strong_self = Self::try_get_strong(weak_self)?;
                (strong_self.inner.lock().time_has_come_for_request)(request)?;
            }

            _ = cancellation_token => {
                let strong_self = Self::try_get_strong(weak_self)?;
                let mut inner = strong_self.inner.lock();
                (inner.time_has_come_for_request)(request.clone())?;
                if let Some(position) = inner.requests.iter().position(|stored_request| *stored_request == request) {
                    inner.requests.remove(position);
                }

                bail!(OPERATION_CANCELED_MSG)
            }
        };

        Ok(())
    }

    fn try_get_strong(
        weak_timeout_manager: Weak<RequestsTimeoutManager>,
    ) -> Result<Arc<RequestsTimeoutManager>> {
        weak_timeout_manager.upgrade().with_context(|| {
            let error_message = "Unable to upgrade weak reference to RequestsTimeoutManager instance. Probably it's dropped";
           log::info!("{}", error_message);
            anyhow!(error_message)
        })
    }

    pub fn register_trigger_on_more_or_equals(
        &self,
        available_requests_count_threshold: usize,
        handler: Box<dyn FnMut() -> Result<()> + Send>,
    ) -> Result<()> {
        let inner = self.inner.lock();
        inner.check_threshold(available_requests_count_threshold)?;
        inner
            .more_or_equals_available_requests_count_trigger_scheduler
            .register_trigger(available_requests_count_threshold, Mutex::new(handler));

        Ok(())
    }

    pub fn register_trigger_on_less_or_equals(
        &self,
        available_requests_count_threshold: usize,
        handler: Box<dyn Fn() -> Result<()> + Send>,
    ) -> Result<()> {
        let mut inner = self.inner.lock();

        inner.check_threshold(available_requests_count_threshold)?;
        let trigger =
            LessOrEqualsRequestsCountTrigger::new(available_requests_count_threshold, handler);
        inner
            .less_or_equals_requests_count_triggers
            .push(Box::new(trigger));

        Ok(())
    }

    pub fn register_trigger_on_every_change(
        &self,
        handler: Box<dyn Fn(usize) -> Result<()> + Send>,
    ) {
        let mut inner = self.inner.lock();

        let trigger = EveryRequestsCountChangeTrigger::new(handler);
        inner
            .less_or_equals_requests_count_triggers
            .push(Box::new(trigger));
    }
}

#[cfg(test)]
mod test {
    use crate::exchanges::timeouts::requests_timeout_manager_factory::{
        RequestTimeoutArguments, RequestsTimeoutManagerFactory,
    };
    use chrono::Utc;

    use super::*;
    use rstest::{fixture, rstest};

    #[fixture]
    fn timeout_manager() -> Arc<RequestsTimeoutManager> {
        let requests_per_period = 5;
        let exchange_account_id = ExchangeAccountId::new("test_exchange_account_id".into(), 0);
        RequestsTimeoutManagerFactory::from_requests_per_period(
            RequestTimeoutArguments::from_requests_per_minute(requests_per_period),
            exchange_account_id,
        )
    }

    mod try_reserve_group {
        use super::*;

        #[rstest]
        fn when_can_reserve(timeout_manager: Arc<RequestsTimeoutManager>) -> Result<()> {
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

            assert_eq!(timeout_manager.inner.lock().pre_reserved_groups.len(), 2);
            let inner = timeout_manager.inner.lock();
            let first_group = inner.pre_reserved_groups.first().expect("in test");

            assert_eq!(first_group.id, first_group_id.expect("in test"));
            assert_eq!(first_group.group_type, group_type);
            assert_eq!(first_group.pre_reserved_requests_count, 3);

            let second_group = inner.pre_reserved_groups[1].clone();

            assert_eq!(second_group.id, second_group_id.expect("in test"));
            assert_eq!(second_group.group_type, group_type);
            assert_eq!(second_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        fn not_enough_requests(timeout_manager: Arc<RequestsTimeoutManager>) -> Result<()> {
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

            assert_eq!(timeout_manager.inner.lock().pre_reserved_groups.len(), 2);
            let inner = timeout_manager.inner.lock();
            let first_group = inner.pre_reserved_groups.first().expect("in test");

            assert_eq!(first_group.id, first_group_id.expect("in test"));
            assert_eq!(first_group.group_type, group_type);
            assert_eq!(first_group.pre_reserved_requests_count, 3);

            let second_group = inner.pre_reserved_groups[1].clone();

            assert_eq!(second_group.id, second_group_id.expect("in test"));
            assert_eq!(second_group.group_type, group_type);
            assert_eq!(second_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        fn when_reserve_after_remove(timeout_manager: Arc<RequestsTimeoutManager>) -> Result<()> {
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

            assert_eq!(timeout_manager.inner.lock().pre_reserved_groups.len(), 1);
            let inner = timeout_manager.inner.lock();
            let group = inner.pre_reserved_groups.first().expect("in test");

            assert_eq!(group.id, first_group_id.expect("in test"));
            assert_eq!(group.group_type, group_type);
            assert_eq!(group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_requests_reserved_and_not_enough_requests(
            timeout_manager: Arc<RequestsTimeoutManager>,
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

            assert_eq!(timeout_manager.inner.lock().requests.len(), 1);

            let inner = timeout_manager.inner.lock();
            let group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(group.id, first_group_id.expect("in test"));
            assert_eq!(group.group_type, group_type);
            assert_eq!(group.pre_reserved_requests_count, 3);

            Ok(())
        }
    }

    mod remove_group {
        use super::*;

        #[rstest]
        fn group_exists(timeout_manager: Arc<RequestsTimeoutManager>) -> Result<()> {
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

            assert_eq!(timeout_manager.inner.lock().pre_reserved_groups.len(), 1);

            let inner = timeout_manager.inner.lock();
            let group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(group.id, first_group_id.expect("in test"));
            assert_eq!(group.group_type, group_type);
            assert_eq!(group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn group_not_exists(timeout_manager: Arc<RequestsTimeoutManager>) -> Result<()> {
            // Arrange
            let current_time = Utc::now();

            // Act
            let removing_result =
                timeout_manager.remove_group(RequestGroupId::generate(), current_time)?;

            // Assert
            assert!(!removing_result);

            Ok(())
        }
    }

    mod try_reserve_instant {
        use crate::infrastructure::init_lifetime_manager;

        use super::*;

        #[rstest]
        fn there_are_spare_requests_true(
            timeout_manager: Arc<RequestsTimeoutManager>,
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

            let inner = timeout_manager.inner.lock();

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert!(first_request.group_id.is_none());

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn there_are_requests_from_future_false(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
            let current_time = Utc::now();
            let before_now = current_time - Duration::seconds(59);

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_now,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_now,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_now,
                CancellationToken::default(),
            )?;

            // Act
            let reserve_result = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(!reserve_result);

            let inner = timeout_manager.inner.lock();
            assert_eq!(inner.requests.len(), 3);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time - Duration::seconds(59)
            );
            assert!(first_request.group_id.is_none());

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time - Duration::seconds(59)
            );
            assert!(second_request.group_id.is_none());

            let third_request = inner.requests[2].clone();
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
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
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

            let inner = timeout_manager.inner.lock();

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert!(first_request.group_id.is_none());

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[rstest]
        fn outdated_request_get_removed(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
            let current_time = Utc::now();
            let before_now = current_time - Duration::seconds(71);

            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                before_now - Duration::seconds(1),
                None,
            )?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, before_now, None)?;

            let inner = timeout_manager.inner.lock();

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time - Duration::seconds(72)
            );
            assert!(first_request.group_id.is_none());

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time - Duration::seconds(71)
            );
            assert!(second_request.group_id.is_none());

            drop(inner);

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

            let inner = timeout_manager.inner.lock();

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert!(first_request.group_id.is_none());

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert!(second_request.group_id.is_none());

            Ok(())
        }

        #[rstest]
        fn when_remove_group_which_blocked_last_request(
            timeout_manager: Arc<RequestsTimeoutManager>,
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

            let inner = timeout_manager.inner.lock();

            let requests_len = inner.requests.len();
            assert_eq!(requests_len, 2);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_cant_reserve_request_in_common_queue_because_of_group_reservations(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 3)?;
            assert!(first_group_id.is_some());

            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            timeout_manager.try_reserve_instant(RequestType::CreateOrder, current_time, None)?;
            assert_eq!(timeout_manager.inner.lock().requests.len(), 2);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;

            // Assert
            assert!(!reserved_instant);

            let inner = timeout_manager.inner.lock();

            let requests_len = inner.requests.len();
            assert_eq!(requests_len, 2);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_cant_reserve_request_without_group(
            timeout_manager: Arc<RequestsTimeoutManager>,
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
            assert_eq!(timeout_manager.inner.lock().requests.len(), 2);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(reserved_instant);

            let inner = timeout_manager.inner.lock();

            let requests_len = inner.requests.len();
            assert_eq!(requests_len, 3);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_cant_reserve_request_in_group(
            timeout_manager: Arc<RequestsTimeoutManager>,
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

            assert_eq!(timeout_manager.inner.lock().requests.len(), 5);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(!reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 5);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_recapturing_vacant_request_in_group_after_exhaustion(
            timeout_manager: Arc<RequestsTimeoutManager>,
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

            assert_eq!(timeout_manager.inner.lock().requests.len(), 5);

            let delay_to_next_time_period = Duration::milliseconds(1);
            let period_duration = timeout_manager.inner.lock().period_duration;

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time + period_duration + delay_to_next_time_period,
                first_group_id,
            )?;

            // Assert
            assert!(reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);
            assert_eq!(inner.pre_reserved_groups.len(), 1);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 3);

            Ok(())
        }

        #[rstest]
        fn when_trying_reserve_without_group(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            let current_time = Utc::now();

            // TODO test logger event here

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                Some(RequestGroupId::generate()),
            )?;

            // Assert
            assert!(reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);
            assert!(inner.pre_reserved_groups.is_empty());

            Ok(())
        }

        #[rstest]
        fn when_another_group_occupied_last_requests(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.inner.lock().requests_per_period = 4;
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

            assert_eq!(timeout_manager.inner.lock().requests.len(), 2);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(!reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 2);

            assert_eq!(inner.pre_reserved_groups.len(), 2);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            let second_reserved_group = inner.pre_reserved_groups[1].clone();
            assert_eq!(second_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(second_reserved_group.group_type, group_type);
            assert_eq!(second_reserved_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn when_there_is_request_in_the_future_time(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.inner.lock().requests_per_period = 4;
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

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CancelOrder,
                current_time,
                CancellationToken::default(),
            )?;
            let first_reserve_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;
            assert!(first_reserve_attempt);

            assert_eq!(timeout_manager.inner.lock().requests.len(), 4);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 5);

            assert_eq!(inner.pre_reserved_groups.len(), 1);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            Ok(())
        }

        #[rstest]
        fn when_group_has_more_requests_then_preferred_count(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.inner.lock().requests_per_period = 4;
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

            assert_eq!(timeout_manager.inner.lock().requests.len(), 4);

            // Act
            let reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                second_group_id,
            )?;

            // Assert
            assert!(!reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 4);

            assert_eq!(inner.pre_reserved_groups.len(), 2);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            let second_reserved_group = inner.pre_reserved_groups[1].clone();
            assert_eq!(second_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(second_reserved_group.group_type, group_type);
            assert_eq!(second_reserved_group.pre_reserved_requests_count, 1);

            Ok(())
        }
    }

    mod reserve_when_available {
        use crate::infrastructure::init_lifetime_manager;

        use super::*;

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn no_current_requests_true(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 1;
            let current_time = Utc::now();

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn only_outdated_requests_true(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 1;
            let current_time = Utc::now();

            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time - Duration::seconds(61),
                None,
            )?;

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);

            let request = inner.requests.first().expect("in test");
            assert_eq!(request.request_type, RequestType::CreateOrder);
            assert_eq!(
                request.allowed_start_time,
                current_time - Duration::seconds(61)
            );
            assert_eq!(request.group_id, None);
            drop(inner);

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn there_are_spare_requests_in_the_last_interval_now_after_last_request_date_time(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 3;
            let current_time = Utc::now();

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time - Duration::seconds(35),
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time - Duration::seconds(10),
                CancellationToken::default(),
            )?;

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 3);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time - Duration::seconds(35)
            );
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time - Duration::seconds(10)
            );
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, current_time);
            assert_eq!(third_request.group_id, None);

            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn there_are_max_requests_in_current_period_in_past(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 3;
            let current_time = Utc::now();
            let before_current = current_time - Duration::seconds(35);

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 3);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, before_current);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, before_current);
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, before_current);
            assert_eq!(third_request.group_id, None);

            drop(inner);

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 4);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, before_current,);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, before_current,);
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, before_current);
            assert_eq!(third_request.group_id, None);

            let next_period_delay = Duration::milliseconds(1);
            let fourth_request = inner.requests[3].clone();
            assert_eq!(fourth_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                fourth_request.allowed_start_time,
                current_time + Duration::seconds(25) + next_period_delay
            );
            assert_eq!(fourth_request.group_id, None);

            assert_eq!(
                available_start_time,
                current_time + Duration::seconds(25) + next_period_delay
            );
            assert_eq!(delay, Duration::seconds(25) + next_period_delay);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn there_are_max_requests_in_current_period_in_future(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 3;
            let current_time = Utc::now();
            let next_period_delay = Duration::milliseconds(1);
            let before_current = current_time - Duration::seconds(25) - next_period_delay;

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 7);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time + Duration::seconds(-25) - next_period_delay
            );
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time + Duration::seconds(-25) - next_period_delay
            );
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                third_request.allowed_start_time,
                current_time + Duration::seconds(-25) - next_period_delay
            );
            assert_eq!(third_request.group_id, None);

            let fourth_request = inner.requests[3].clone();
            assert_eq!(fourth_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                fourth_request.allowed_start_time,
                current_time + Duration::seconds(35)
            );
            assert_eq!(fourth_request.group_id, None);

            let fifth_request = inner.requests[4].clone();
            assert_eq!(fifth_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                fifth_request.allowed_start_time,
                current_time + Duration::seconds(35)
            );
            assert_eq!(fifth_request.group_id, None);

            let sixth_request = inner.requests[5].clone();
            assert_eq!(sixth_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                sixth_request.allowed_start_time,
                current_time + Duration::seconds(35)
            );
            assert_eq!(sixth_request.group_id, None);

            let seventh_request = inner.requests[6].clone();
            assert_eq!(seventh_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                seventh_request.allowed_start_time,
                current_time + Duration::seconds(95) + next_period_delay
            );
            assert_eq!(seventh_request.group_id, None);

            assert_eq!(
                available_start_time,
                current_time + Duration::seconds(95) + next_period_delay
            );
            assert_eq!(delay, Duration::seconds(95) + next_period_delay);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn there_are_no_max_requests_in_current_period_in_future(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
            let current_time = Utc::now();
            let next_period_delay = Duration::milliseconds(1);
            let before_current = current_time - Duration::seconds(25) - next_period_delay;

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 4);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                first_request.allowed_start_time,
                current_time - Duration::seconds(25) - next_period_delay
            );
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time - Duration::seconds(25) - next_period_delay
            );
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                third_request.allowed_start_time,
                current_time + Duration::seconds(35)
            );
            assert_eq!(third_request.group_id, None);

            let fourth_request = inner.requests[3].clone();
            assert_eq!(fourth_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                fourth_request.allowed_start_time,
                current_time + Duration::seconds(35)
            );
            assert_eq!(fourth_request.group_id, None);

            assert_eq!(available_start_time, current_time + Duration::seconds(35));
            assert_eq!(delay, Duration::seconds(35));

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_cancellation_token(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
            let current_time = Utc::now();

            // Act
            let (_, available_start_time, delay) = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_cancel_at_beginning(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
            let current_time = Utc::now();
            let cancellation_token = CancellationToken::default();
            cancellation_token.cancel();

            // Act
            let (future_handler, available_start_time, delay) =
                timeout_manager.clone().reserve_when_available(
                    RequestType::CreateOrder,
                    current_time,
                    cancellation_token,
                )?;

            let cancelled = future_handler.await?.into_result();

            // Assert
            assert!(cancelled.is_err());
            let inner = timeout_manager.inner.lock();

            assert!(inner.requests.is_empty());
            assert_eq!(available_start_time, current_time);
            assert_eq!(delay, Duration::zero());

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_cancel_after_two_seconds(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            timeout_manager.inner.lock().requests_per_period = 2;
            let current_time = Utc::now();
            let next_period_delay = Duration::milliseconds(1);
            let before_current = current_time - Duration::seconds(55) - next_period_delay;

            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                before_current,
                CancellationToken::default(),
            )?;

            let cancellation_token = CancellationToken::new();

            // Scope to make future drop
            {
                // Act
                let (mut future_handler, available_start_time, delay) =
                    timeout_manager.clone().reserve_when_available(
                        RequestType::CreateOrder,
                        current_time,
                        cancellation_token.create_linked_token(),
                    )?;

                let inner = timeout_manager.inner.lock();

                assert_eq!(inner.requests.len(), 3);

                let first_request = inner.requests.first().expect("in test");
                assert_eq!(first_request.request_type, RequestType::CreateOrder);
                assert_eq!(first_request.allowed_start_time, before_current);
                assert_eq!(first_request.group_id, None);

                let second_request = inner.requests[1].clone();
                assert_eq!(second_request.request_type, RequestType::CreateOrder);
                assert_eq!(second_request.allowed_start_time, before_current);
                assert_eq!(second_request.group_id, None);

                let third_request = inner.requests[2].clone();
                assert_eq!(third_request.request_type, RequestType::CreateOrder);
                assert_eq!(
                    third_request.allowed_start_time,
                    current_time + Duration::seconds(5)
                );
                assert_eq!(third_request.group_id, None);

                drop(inner);

                let sleep_future = sleep(std::time::Duration::from_millis(1000));
                tokio::select! {
                    _ = sleep_future => {
                        cancellation_token.cancel();

                        let cancelled = future_handler.await?.into_result();

                        dbg!(&cancelled);
                        // Assert
                        assert!(cancelled.is_err());

                        assert_eq!(available_start_time, current_time + Duration::seconds(5));
                        assert_eq!(delay, Duration::seconds(5));
                    }

                    _ = &mut future_handler => {
                        bail!("Future completed")
                    }
                };
            }

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 2);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, before_current);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, before_current);
            assert_eq!(second_request.group_id, None);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_reserved_all_request_in_group_and_one_request_available(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();

            let group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 4)?;
            assert!(group_id.is_some());

            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;

            // Act
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::new(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 5);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, group_id);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert_eq!(second_request.group_id, group_id);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, current_time);
            assert_eq!(third_request.group_id, None);

            let fourth_request = inner.requests[3].clone();
            assert_eq!(fourth_request.request_type, RequestType::CreateOrder);
            assert_eq!(fourth_request.allowed_start_time, current_time);
            assert_eq!(fourth_request.group_id, group_id);

            let fifth_request = inner.requests[4].clone();
            assert_eq!(fifth_request.request_type, RequestType::CreateOrder);
            assert_eq!(fifth_request.allowed_start_time, current_time);
            assert_eq!(fifth_request.group_id, group_id);

            assert_eq!(inner.pre_reserved_groups.len(), 1);
            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 4);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_pre_reserved_four_slots_in_group_and_one_request(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();

            let group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 4)?;
            assert!(group_id.is_some());

            let reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(reserve_instant_attempt);

            // Act
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::new(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 2);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            let delay_to_next_time_period = Duration::milliseconds(1);
            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                second_request.allowed_start_time,
                current_time + inner.period_duration + delay_to_next_time_period
            );
            assert_eq!(second_request.group_id, None);

            assert_eq!(inner.pre_reserved_groups.len(), 1);
            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 4);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_pre_reserved_four_slots_in_group_and_one_request_in_group_and_one_request_without_group(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();

            let group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 4)?;
            assert!(group_id.is_some());

            let first_reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            assert!(first_reserve_instant_attempt);
            let second_reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(second_reserve_instant_attempt);

            // Act
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::new(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 3);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert_eq!(second_request.group_id, group_id);

            let delay_to_next_time_period = Duration::milliseconds(1);
            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                third_request.allowed_start_time,
                current_time + inner.period_duration + delay_to_next_time_period
            );
            assert_eq!(third_request.group_id, None);

            assert_eq!(inner.pre_reserved_groups.len(), 1);
            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 4);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn with_pre_reserved_all_slots_in_group(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let current_time = Utc::now();
            let group_type = "GroupType".to_owned();

            let group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 5)?;
            assert!(group_id.is_some());

            let first_reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            assert!(first_reserve_instant_attempt);
            let second_reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            assert!(second_reserve_instant_attempt);
            let third_reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            assert!(third_reserve_instant_attempt);
            let fourth_reserve_instant_attempt = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                group_id,
            )?;
            assert!(fourth_reserve_instant_attempt);

            // Act
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::new(),
            )?;

            // Assert
            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 5);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, group_id);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert_eq!(second_request.group_id, group_id);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, current_time);
            assert_eq!(third_request.group_id, group_id);

            let fourth_request = inner.requests[3].clone();
            assert_eq!(fourth_request.request_type, RequestType::CreateOrder);
            assert_eq!(fourth_request.allowed_start_time, current_time);
            assert_eq!(fourth_request.group_id, group_id);

            let delay_to_next_time_period = Duration::milliseconds(1);
            let fifth_request = inner.requests[4].clone();
            assert_eq!(fifth_request.request_type, RequestType::CreateOrder);
            assert_eq!(
                fifth_request.allowed_start_time,
                current_time + inner.period_duration + delay_to_next_time_period
            );
            assert_eq!(fifth_request.group_id, None);

            assert_eq!(inner.pre_reserved_groups.len(), 1);
            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 5);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn when_all_requests_pre_reserved_in_groups(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let group_type = "GroupType".to_owned();
            timeout_manager.inner.lock().requests_per_period = 4;
            let current_time = Utc::now();

            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            let second_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            assert!(first_group_id.is_some());
            assert!(second_group_id.is_some());

            // Act
            let first_reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            let _ = timeout_manager.clone().reserve_when_available(
                RequestType::CreateOrder,
                current_time,
                CancellationToken::default(),
            )?;
            let second_reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            let third_reserved_instant = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                first_group_id,
            )?;

            // Assert
            assert!(!first_reserved_instant);
            assert!(second_reserved_instant);
            assert!(third_reserved_instant);

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 3);

            assert_eq!(inner.pre_reserved_groups.len(), 2);

            let first_reserved_group = inner.pre_reserved_groups.first().expect("in test");
            assert_eq!(first_reserved_group.id, first_group_id.expect("in test"));
            assert_eq!(first_reserved_group.group_type, group_type);
            assert_eq!(first_reserved_group.pre_reserved_requests_count, 2);

            let second_reserved_group = inner.pre_reserved_groups[1].clone();
            assert_eq!(second_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(second_reserved_group.group_type, group_type);
            assert_eq!(second_reserved_group.pre_reserved_requests_count, 2);

            let third_reserved_group = inner.pre_reserved_groups[1].clone();
            assert_eq!(third_reserved_group.id, second_group_id.expect("in test"));
            assert_eq!(third_reserved_group.group_type, group_type);
            assert_eq!(third_reserved_group.pre_reserved_requests_count, 2);

            Ok(())
        }
    }

    mod triggers {
        use parking_lot::Mutex;

        use crate::infrastructure::init_lifetime_manager;

        use super::*;
        use std::sync::Arc;

        #[fixture]
        fn timeout_manager() -> Arc<RequestsTimeoutManager> {
            let requests_per_period = 5;
            let exchange_account_id = ExchangeAccountId::new("test_exchange_account_id".into(), 0);
            RequestsTimeoutManagerFactory::from_requests_per_period(
                RequestTimeoutArguments::new(requests_per_period, Duration::milliseconds(1)),
                exchange_account_id,
            )
        }

        struct CallCounter {
            count: usize,
        }

        impl CallCounter {
            fn new() -> Self {
                Self { count: 0 }
            }

            fn call(&mut self) {
                self.count += 1;
            }

            fn count(&self) -> usize {
                self.count
            }
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn calls_count_zero_when_only_reserve_instant(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            // Arrange
            let call_counter = Arc::new(Mutex::new(CallCounter::new()));
            let cloned_counter = call_counter.clone();
            timeout_manager.register_trigger_on_more_or_equals(
                3,
                Box::new(move || {
                    cloned_counter.lock().call();
                    Ok(())
                }),
            )?;
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
            sleep(std::time::Duration::from_millis(50)).await;

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 2);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert_eq!(second_request.group_id, None);

            assert!(first_reserved);
            assert!(second_reserved);

            assert_eq!(call_counter.lock().count(), 0);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn calls_count_one_when_only_reserve_instant(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let call_counter = Arc::new(Mutex::new(CallCounter::new()));
            let cloned_counter = call_counter.clone();
            timeout_manager.register_trigger_on_more_or_equals(
                3,
                Box::new(move || {
                    cloned_counter.lock().call();
                    Ok(())
                }),
            )?;
            let current_time = Utc::now();

            // Act
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(first_reserved);
            let second_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(second_reserved);
            let third_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(third_reserved);

            // Assert
            sleep(std::time::Duration::from_millis(50)).await;

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 3);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, current_time);
            assert_eq!(third_request.group_id, None);

            assert_eq!(call_counter.lock().count(), 1);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn calls_count_two_when_one_extra_request(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let call_counter = Arc::new(Mutex::new(CallCounter::new()));
            let cloned_counter = call_counter.clone();
            timeout_manager.register_trigger_on_more_or_equals(
                3,
                Box::new(move || {
                    cloned_counter.lock().call();
                    Ok(())
                }),
            )?;
            let current_time = Utc::now();

            // Act
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(first_reserved);
            let second_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(second_reserved);
            let third_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(third_reserved);
            let fourth_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(fourth_reserved);

            // Assert
            sleep(std::time::Duration::from_millis(50)).await;

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 4);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            let second_request = inner.requests[1].clone();
            assert_eq!(second_request.request_type, RequestType::CreateOrder);
            assert_eq!(second_request.allowed_start_time, current_time);
            assert_eq!(second_request.group_id, None);

            let third_request = inner.requests[2].clone();
            assert_eq!(third_request.request_type, RequestType::CreateOrder);
            assert_eq!(third_request.allowed_start_time, current_time);
            assert_eq!(third_request.group_id, None);

            let fourth_request = inner.requests[2].clone();
            assert_eq!(fourth_request.request_type, RequestType::CreateOrder);
            assert_eq!(fourth_request.allowed_start_time, current_time);
            assert_eq!(fourth_request.group_id, None);

            assert_eq!(call_counter.lock().count(), 2);

            Ok(())
        }

        #[rstest]
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn calls_count_one_when_there_are_prereserved_group(
            timeout_manager: Arc<RequestsTimeoutManager>,
        ) -> Result<()> {
            let _ = init_lifetime_manager();

            // Arrange
            let group_type = "GroupType".to_owned();
            let call_counter = Arc::new(Mutex::new(CallCounter::new()));
            let cloned_counter = call_counter.clone();
            timeout_manager.register_trigger_on_more_or_equals(
                3,
                Box::new(move || {
                    cloned_counter.lock().call();
                    Ok(())
                }),
            )?;
            let current_time = Utc::now();

            // Act
            let first_group_id =
                timeout_manager.try_reserve_group(group_type.clone(), current_time, 2)?;
            assert!(first_group_id.is_some());
            let first_reserved = timeout_manager.try_reserve_instant(
                RequestType::CreateOrder,
                current_time,
                None,
            )?;
            assert!(first_reserved);

            // Assert
            sleep(std::time::Duration::from_millis(50)).await;

            let inner = timeout_manager.inner.lock();

            assert_eq!(inner.requests.len(), 1);

            let first_request = inner.requests.first().expect("in test");
            assert_eq!(first_request.request_type, RequestType::CreateOrder);
            assert_eq!(first_request.allowed_start_time, current_time);
            assert_eq!(first_request.group_id, None);

            assert_eq!(call_counter.lock().count(), 1);

            Ok(())
        }
    }
}
