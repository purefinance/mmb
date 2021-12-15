use futures::future::ready;
use futures::future::Either;
use futures::FutureExt;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::{CompletionReason, FutureOutcome};
use mmb_utils::DateTime;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;

use anyhow::Result;
use chrono::Utc;

use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::general::request_type::RequestType;
use crate::core::exchanges::timeouts::requests_timeout_manager::{
    RequestGroupId, RequestsTimeoutManager,
};

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
    ) -> Result<Option<RequestGroupId>> {
        self.inner[&exchange_account_id].try_reserve_group(group_type, now(), requests_count)
    }

    pub fn remove_group(
        &self,
        exchange_account_id: ExchangeAccountId,
        group_id: RequestGroupId,
    ) -> Result<bool> {
        self.inner[&exchange_account_id].remove_group(group_id, now())
    }

    pub fn try_reserve_instant(
        &self,
        exchange_account_id: ExchangeAccountId,
        request_type: RequestType,
    ) -> Result<bool> {
        self.inner[&exchange_account_id].try_reserve_instant(request_type, now(), None)
    }

    pub fn try_reserve_group_instant(
        &self,
        exchange_account_id: ExchangeAccountId,
        request_type: RequestType,
        pre_reserved_group_id: Option<RequestGroupId>,
    ) -> Result<bool> {
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
    ) -> Result<impl Future<Output = FutureOutcome> + Send + Sync> {
        let inner = (&self.inner[&exchange_account_id]).clone();

        const ERROR_MSG: &str = "Failed waiting in method TimeoutManager::reserve_when_available";
        let convert = |handle: JoinHandle<FutureOutcome>| {
            handle.map(|res| match res {
                Ok(future_outcome) => future_outcome,
                // Only panic can happen here and only in case if spawn_future() panicked itself
                Err(error) => {
                    log::error!("Future in reserve_when_available got error: {}", error);
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
            let result = inner.reserve_when_available(request_type, now, cancellation_token)?;
            return Ok(Either::Left(convert(result.0)));
        }

        if inner.try_reserve_instant(request_type, now, pre_reservation_group_id)? {
            return Ok(Either::Right(ready(FutureOutcome::new(
                "spawn_future() for try_reserve_instant".to_owned(),
                Uuid::new_v4(),
                CompletionReason::CompletedSuccessfully,
            ))));
        }

        let result = inner.reserve_when_available(request_type, now, cancellation_token)?;
        Ok(Either::Left(convert(result.0)))
    }
}

pub fn now() -> DateTime {
    Utc::now()
}
