use crate::core::exchanges::common::TradePlaceAccount;
use crate::core::orders::order::{
    ClientOrderId, OrderHeader, OrderSimpleProps, OrderSnapshot, OrderStatus,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, BorrowMut};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderRef(Arc<RwLock<OrderSnapshot>>);

impl OrderRef {
    /// Lock order for read and provide copy properties or check some conditions
    pub fn fn_ref<T>(&self, f: impl FnOnce(&OrderSnapshot) -> T) -> T {
        f(self.0.read().borrow())
    }

    /// Lock order for write and provide mutate state of order
    pub fn fn_mut<T>(&self, mut f: impl FnMut(&mut OrderSnapshot) -> T) -> T {
        f(self.0.write().borrow_mut())
    }

    pub fn trade_place_account(&self) -> TradePlaceAccount {
        self.fn_ref(|x| {
            TradePlaceAccount::new(
                x.header.exchange_account_id.clone(),
                x.header.currency_pair.clone(),
            )
        })
    }

    pub fn price(&self) -> Decimal {
        self.fn_ref(|x| x.props.price())
    }
    pub fn amount(&self) -> Decimal {
        self.fn_ref(|x| x.header.amount)
    }
    pub fn status(&self) -> OrderStatus {
        self.fn_ref(|x| x.props.status)
    }
}

pub struct OrdersPool {
    orders: DashMap<ClientOrderId, Arc<RwLock<OrderSnapshot>>>,
}

impl OrdersPool {
    pub fn new() -> Arc<Self> {
        Arc::new(OrdersPool {
            orders: DashMap::with_capacity(100),
        })
    }

    /// Insert specified `OrderSnapshot` in order pool.
    /// Return true if there is already order with specified client order id in pool (new order replace old order)
    pub fn add_snapshot(&self, snapshot: Arc<RwLock<OrderSnapshot>>) -> bool {
        let client_order_id = snapshot.read().header.client_order_id.clone();
        self.orders.insert(client_order_id, snapshot).is_some()
    }

    /// Create `OrderSnapshot` by specified `OrderHeader` + order price with default other properties and insert it in order pool.
    /// Return true if there is already order with specified client order id in pool (new order replace old order)
    pub fn add_simple(&self, header: Arc<OrderHeader>, price: Option<Decimal>) -> bool {
        let snapshot = Arc::new(RwLock::new(OrderSnapshot {
            props: OrderSimpleProps::new(header.client_order_id.clone(), price),
            header,
            fills: Default::default(),
            status_history: Default::default(),
            internal_props: Default::default(),
        }));

        self.add_snapshot(snapshot)
    }

    /// Remove specified order from pool
    pub fn remove(&self, client_order_id: &ClientOrderId) {
        let _ = self.orders.remove(client_order_id);
    }

    pub fn get(&self, client_order_id: &ClientOrderId) -> Option<OrderRef> {
        self.orders
            .get(client_order_id)
            .map(|x| OrderRef(x.clone()))
    }
}
