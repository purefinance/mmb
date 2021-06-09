use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;

use anyhow::Result;
use chrono::{Duration, Utc};

use crate::core::exchanges::cancellation_token::CancellationToken;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::general::request_type::RequestType;
use crate::core::exchanges::timeouts::requests_timeout_manager::{
    RequestGroupId, RequestsTimeoutManager,
};
use crate::core::DateTime;

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
        exchange_account_id: &ExchangeAccountId,
        requests_count: usize,
        group_type: String,
    ) -> Result<Option<RequestGroupId>> {
        self.inner[exchange_account_id].try_reserve_group(group_type, now(), requests_count)
    }

    pub fn remove_group(
        &self,
        exchange_account_id: &ExchangeAccountId,
        group_id: RequestGroupId,
    ) -> Result<bool> {
        self.inner[exchange_account_id].remove_group(group_id, now())
    }

    pub fn try_reserve_instant(
        &self,
        exchange_account_id: &ExchangeAccountId,
        request_type: RequestType,
    ) -> Result<bool> {
        self.inner[exchange_account_id].try_reserve_instant(request_type, now(), None)
    }

    pub fn try_reserve_group_instant(
        &self,
        exchange_account_id: &ExchangeAccountId,
        request_type: RequestType,
        pre_reserved_group_id: Option<RequestGroupId>,
    ) -> Result<bool> {
        self.inner[exchange_account_id].try_reserve_instant(
            request_type,
            now(),
            pre_reserved_group_id,
        )
    }

    pub fn reserve_when_available(
        &self,
        exchange_account_id: &ExchangeAccountId,
        request_type: RequestType,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
        // FIXME Maybe delete Datime and Duration at all?
    ) -> Result<(JoinHandle<Result<()>>, DateTime, Duration)> {
        let inner = &self.inner[exchange_account_id];
        let current_time = now();

        if pre_reservation_group_id.is_none() {
            return inner.clone().reserve_when_available(
                request_type,
                current_time,
                cancellation_token,
            );
        }

        if !inner.try_reserve_instant(request_type, current_time, pre_reservation_group_id)? {
            return inner.clone().reserve_when_available(
                request_type,
                current_time,
                cancellation_token,
            );
        }

        let completed_task = tokio::task::spawn(async { Ok(()) });
        Ok((completed_task, now(), Duration::zero()))
    }
}

pub fn now() -> DateTime {
    Utc::now()
}
