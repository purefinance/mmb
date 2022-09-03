use crate::exchanges::general::exchange::Exchange;
use crate::lifecycle::trading_engine::Service;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use mmb_domain::market::ExchangeAccountId;
use mmb_domain::order::pool::OrderRef;
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
        let deadline = Utc::now() - chrono::Duration::minutes(30);
        self.exchanges.iter().for_each(|pair| {
            cleanup(&pair.orders.cache_by_exchange_id, deadline);
            cleanup(&pair.orders.cache_by_client_id, deadline);
        });
    }
}

fn cleanup<T>(orders: &DashMap<T, OrderRef>, deadline: DateTime<Utc>)
where
    T: Eq + Hash,
{
    orders
        .retain(|_, v| v.fn_ref(|x| x.props.finished_time.map(|x| x >= deadline).unwrap_or(true)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use mmb_domain::market::CurrencyPair;
    use mmb_domain::order::pool::OrdersPool;
    use mmb_domain::order::snapshot::{
        ClientOrderId, OrderExecutionType, OrderHeader, OrderSide, OrderStatus, OrderType,
    };
    use rstest::rstest;
    use rust_decimal_macros::dec;

    #[rstest]
    #[timeout(std::time::Duration::from_millis(200))]
    pub fn test_cleanup_when_time_is_up() {
        let k: ClientOrderId = "test".into();

        let pool = OrdersPool::new();
        let now = Utc::now();
        let header = OrderHeader::new(
            k,
            now,
            ExchangeAccountId::new("Binance", 0),
            CurrencyPair::from_codes("a".into(), "b".into()),
            OrderType::Limit,
            OrderSide::Buy,
            dec!(1),
            OrderExecutionType::None,
            None,
            None,
            "".to_string(),
        );
        let order_ref = pool.add_simple_initial(header, Some(dec!(0.5)), None);
        order_ref.fn_mut(|x| x.set_status(OrderStatus::Completed, now));

        // deadline has arrived
        let deadline = now + Duration::minutes(1);
        cleanup(&pool.cache_by_client_id, deadline);
        assert!(pool.cache_by_client_id.is_empty());
    }

    #[rstest]
    #[timeout(std::time::Duration::from_millis(200))]
    pub fn test_cleanup_if_not_ehough_time_has_passed() {
        let k: ClientOrderId = "test".into();

        let pool = OrdersPool::new();
        let now = Utc::now();
        let header = OrderHeader::new(
            k,
            now,
            ExchangeAccountId::new("Binance", 0),
            CurrencyPair::from_codes("a".into(), "b".into()),
            OrderType::Limit,
            OrderSide::Buy,
            dec!(1),
            OrderExecutionType::None,
            None,
            None,
            "".to_string(),
        );
        let order_ref = pool.add_simple_initial(header, Some(dec!(0.5)), None);
        order_ref.fn_mut(|x| x.set_status(OrderStatus::Completed, now));

        // deadline has not arrived
        let deadline = now - Duration::minutes(5);
        cleanup(&pool.cache_by_client_id, deadline);
        assert_eq!(pool.cache_by_client_id.len(), 1);
    }

    #[rstest]
    #[timeout(std::time::Duration::from_millis(200))]
    pub fn test_cleanup_when_order_is_not_completed() {
        let k: ClientOrderId = "test".into();

        let pool = OrdersPool::new();
        let now = Utc::now();
        let header = OrderHeader::new(
            k,
            now,
            ExchangeAccountId::new("Binance", 0),
            CurrencyPair::from_codes("a".into(), "b".into()),
            OrderType::Limit,
            OrderSide::Buy,
            dec!(1),
            OrderExecutionType::None,
            None,
            None,
            "".to_string(),
        );
        pool.add_simple_initial(header, Some(dec!(0.5)), None);

        let deadline = now + Duration::minutes(5);
        cleanup(&pool.cache_by_client_id, deadline);
        assert_eq!(pool.cache_by_client_id.len(), 1);
    }
}
