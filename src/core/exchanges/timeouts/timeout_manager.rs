use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;

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
    pub fn new() -> Arc<Self> {
        Arc::new(TimeoutManager {
            // TODO initialize for all exchanges
            inner: Default::default(),
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
        _exchange_account_id: &ExchangeAccountId,
        _request_type: RequestType,
        _pre_reserved_group_id: Option<RequestGroupId>,
        _cancellation_token: CancellationToken,
    ) -> Result<BoxFuture> {
        // TODO needed implementation in future
        // let inner = &self.inner[exchange_account_id];
        //
        // let now = now();
        // if pre_reserved_group_id.is_none() {
        //     let result = inner.reserve_when_available(request_type, now, cancellation_token)?;
        //     return Ok(Box::new(result.0) as BoxFuture);
        // }
        //
        // if inner.try_reserve_instant(request_type, now, pre_reserved_group_id)? {
        //     return Ok(futures::future::ready(Ok(())) as BoxFuture);
        // }
        //
        // let result = inner.reserve_when_available(request_type, now, cancellation_token)?;
        // Ok(Box::new(result.0) as BoxFuture)
        todo!("Not implemented yet")
    }
}

pub fn now() -> DateTime {
    Utc::now()
}
