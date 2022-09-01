use futures::future::ready;
use futures::future::Either;
use futures::FutureExt;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::{CompletionReason, FutureOutcome, WithExpect};
use mmb_utils::DateTime;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use uuid::Uuid;

use anyhow::Result;
use chrono::Utc;

use crate::exchanges::general::request_type::RequestType;
use crate::exchanges::timeouts::requests_timeout_manager::{
    RequestGroupId, RequestsTimeoutManager,
};
use mmb_domain::market::ExchangeAccountId;

pub type BoxFuture = Box<dyn Future<Output = Result<()>> + Sync + Send>;

pub struct TimeoutManager {
    inner: HashMap<ExchangeAccountId, Arc<RequestsTimeoutManager>>,
}

impl TimeoutManager {
    pub fn new(
        timeout_managers: HashMap<ExchangeAccountId, Arc<RequestsTimeoutManager>>,
    ) -> Arc<Self> {
        Arc::new(TimeoutManager {
            inner: timeout_managers,
        })
    }

    pub fn try_reserve_group(
        &self,
        exchange_account_id: ExchangeAccountId,
        requests_count: usize,
        group_type: String,
    ) -> Option<RequestGroupId> {
        self.inner[&exchange_account_id].try_reserve_group(group_type, now(), requests_count)
    }

    pub fn remove_group(
        &self,
        exchange_account_id: ExchangeAccountId,
        group_id: RequestGroupId,
    ) -> bool {
        self.inner[&exchange_account_id].remove_group(group_id, now())
    }

    pub fn try_reserve_instant(
        &self,
        exchange_account_id: ExchangeAccountId,
        request_type: RequestType,
    ) -> bool {
        self.inner[&exchange_account_id].try_reserve_instant(request_type, now(), None)
    }

    pub fn try_reserve_group_instant(
        &self,
        exchange_account_id: ExchangeAccountId,
        request_type: RequestType,
        pre_reserved_group_id: Option<RequestGroupId>,
    ) -> bool {
        self.inner[&exchange_account_id].try_reserve_instant(
            request_type,
            now(),
            pre_reserved_group_id,
        )
    }

    pub fn reserve_when_available(
        &self,
        exchange_account_id: ExchangeAccountId,
        request_type: RequestType,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) -> impl Future<Output = FutureOutcome> + Send + Sync {
        let inner = (&self.inner[&exchange_account_id]).clone();

        let convert = |handle: JoinHandle<FutureOutcome>| {
            handle.map(|res| match res {
                Ok(future_outcome) => future_outcome,
                // Only panic can happen here and only in case if spawn_future() panicked itself
                Err(err) => {
                    log::error!("Future in reserve_when_available got error: {err}");
                    FutureOutcome::new(
                        "spawn_future() for reserve_when_available".to_owned(),
                        Uuid::new_v4(),
                        CompletionReason::Panicked,
                    )
                }
            })
        };

        let now = now();
        if pre_reservation_group_id.is_none() {
            let result = inner.reserve_when_available(request_type, now, cancellation_token);
            return Either::Left(convert(result.0));
        }

        if inner.try_reserve_instant(request_type, now, pre_reservation_group_id) {
            return Either::Right(ready(FutureOutcome::new(
                "spawn_future() for try_reserve_instant".to_owned(),
                Uuid::new_v4(),
                CompletionReason::CompletedSuccessfully,
            )));
        }

        let result = inner.reserve_when_available(request_type, now, cancellation_token);
        Either::Left(convert(result.0))
    }

    pub fn get_period_duration(&self, exchange_account_id: ExchangeAccountId) -> Duration {
        self.inner
            .get(&exchange_account_id)
            .with_expect(|| format!("Can't find timeout manger for {exchange_account_id}"))
            .get_period_duration()
    }
}

pub fn now() -> DateTime {
    Utc::now()
}
