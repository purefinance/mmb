use crate::exchanges::common::ExchangeAccountId;
use crate::exchanges::general::exchange::Exchange;
use crate::lifecycle::trading_engine::Service;
use crate::orders::pool::OrderRef;
use chrono::Utc;
use dashmap::DashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::oneshot::Receiver;

pub struct CleanupOrdersService {
    exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
}

impl Service for CleanupOrdersService {
    fn name(&self) -> &str {
        "CleanupOrdersService"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<Receiver<anyhow::Result<()>>> {
        None
    }
}

impl CleanupOrdersService {
    pub fn new(exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>) -> Self {
        Self { exchanges }
    }
    pub async fn cleanup_outdated_orders(self: Arc<Self>) {
        self.exchanges.iter().for_each(|pair| {
            Self::cleanup(&pair.orders.cache_by_exchange_id);
            Self::cleanup(&pair.orders.cache_by_client_id);
        });
    }

    fn cleanup<T>(orders: &DashMap<T, OrderRef>)
    where
        T: Eq + Hash,
    {
        let deadline = Utc::now() - chrono::Duration::minutes(30);
        for pair in orders {
            if let Some(time) = pair.value().fn_ref(|x| x.props.finished_time) {
                if time < deadline {
                    orders.remove(pair.key());
                }
            }
        }
    }
}
