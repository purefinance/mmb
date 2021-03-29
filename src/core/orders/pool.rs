use crate::core::exchanges::common::{ExchangeAccountId, TradePlaceAccount};
use crate::core::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderHeader, OrderSimpleProps, OrderSnapshot, OrderStatus,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, BorrowMut};
use std::sync::Arc;

use super::{fill::OrderFill, order::ReservationId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderRef(Arc<RwLock<OrderSnapshot>>);

impl OrderRef {
    /// Lock order for read and provide copy properties or check some conditions
    pub fn fn_ref<T: 'static>(&self, f: impl FnOnce(&OrderSnapshot) -> T) -> T {
        f(self.0.read().borrow())
    }

    /// Lock order for write and provide mutate state of order
    pub fn fn_mut<T: 'static>(&self, mut f: impl FnMut(&mut OrderSnapshot) -> T) -> T {
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
        self.fn_ref(|x| x.price())
    }
    pub fn amount(&self) -> Decimal {
        self.fn_ref(|x| x.header.amount)
    }
    pub fn status(&self) -> OrderStatus {
        self.fn_ref(|x| x.props.status)
    }
    pub fn exchange_order_id(&self) -> Option<ExchangeOrderId> {
        self.fn_ref(|x| x.props.exchange_order_id.clone())
    }
    pub fn client_order_id(&self) -> ClientOrderId {
        self.fn_ref(|x| x.header.client_order_id.clone())
    }
    pub fn exchange_account_id(&self) -> ExchangeAccountId {
        self.fn_ref(|x| x.header.exchange_account_id.clone())
    }
    pub fn reservation_id(&self) -> Option<ReservationId> {
        self.fn_ref(|x| x.header.reservation_id.clone())
    }

    pub fn deep_clone(&self) -> OrderSnapshot {
        self.fn_ref(|order| order.clone())
    }

    pub fn get_fills(&self) -> (Vec<OrderFill>, Decimal) {
        // FIXME is that OK to clone it here?
        self.fn_ref(|order| (order.fills.fills.clone(), order.fills.filled_amount))
    }
}

pub struct OrdersPool {
    pub by_client_id: DashMap<ClientOrderId, OrderRef>,
    pub by_exchange_id: DashMap<ExchangeOrderId, OrderRef>,
    pub not_finished: DashMap<ClientOrderId, OrderRef>,
    _private: (), // field base constructor shouldn't be accessible from other modules
}

impl OrdersPool {
    pub fn new() -> Arc<Self> {
        const ORDERS_INIT_CAPACITY: usize = 100;

        Arc::new(OrdersPool {
            by_client_id: DashMap::with_capacity(ORDERS_INIT_CAPACITY),
            by_exchange_id: DashMap::with_capacity(ORDERS_INIT_CAPACITY),
            not_finished: DashMap::with_capacity(ORDERS_INIT_CAPACITY),
            _private: (),
        })
    }

    /// Insert specified `OrderSnapshot` in order pool.
    pub fn add_snapshot_initial(&self, snapshot: Arc<RwLock<OrderSnapshot>>) {
        let client_order_id = snapshot.read().header.client_order_id.clone();
        let order_ref = OrderRef(snapshot.clone());
        let _ = self
            .by_client_id
            .insert(client_order_id.clone(), order_ref.clone());
        let _ = self.not_finished.insert(client_order_id, order_ref);
    }

    /// Create `OrderSnapshot` by specified `OrderHeader` + order price with default other properties and insert it in order pool.
    pub fn add_simple_initial(&self, header: Arc<OrderHeader>, price: Option<Decimal>) {
        let snapshot = Arc::new(RwLock::new(OrderSnapshot {
            props: OrderSimpleProps::new(price),
            header,
            fills: Default::default(),
            status_history: Default::default(),
            internal_props: Default::default(),
        }));

        self.add_snapshot_initial(snapshot)
    }
}
