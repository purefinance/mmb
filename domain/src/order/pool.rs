use crate::market::CurrencyPair;
use crate::market::ExchangeAccountId;
use crate::order::fill::OrderFill;
use crate::order::snapshot::{
    Amount, ClientOrderId, ExchangeOrderId, OrderHeader, OrderInfoExtensionData, OrderMut,
    OrderSimpleProps, OrderSnapshot, OrderStatus, Price,
};
use crate::order::snapshot::{OrderRole, OrderSide, OrderType};
use dashmap::DashMap;
use mmb_utils::DateTime;
use parking_lot::RwLock;
use std::borrow::{Borrow, BorrowMut};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

pub struct OrderRefData {
    header: OrderHeader,
    data: RwLock<OrderMut>,
}

impl Debug for OrderRefData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "header: {:?} data: {:?}", self.header, self.data)
    }
}

#[derive(Clone, Debug)]
pub struct OrderRef {
    inner: Arc<OrderRefData>,
}

impl PartialEq for OrderRef {
    fn eq(&self, other: &Self) -> bool {
        // Active OrderRef should point to the same OrderRefData
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl OrderRef {
    fn from_snapshot(snapshot: &OrderSnapshot) -> Self {
        Self {
            inner: Arc::new(OrderRefData {
                header: snapshot.header.clone(),
                data: RwLock::new(OrderMut {
                    props: snapshot.props.clone(),
                    fills: snapshot.fills.clone(),
                    status_history: snapshot.status_history.clone(),
                    internal_props: snapshot.internal_props.clone(),
                    extension_data: snapshot.extension_data.clone(),
                }),
            }),
        }
    }

    pub fn header(&self) -> &OrderHeader {
        &self.inner.header
    }

    pub fn exchange_account_id(&self) -> ExchangeAccountId {
        self.header().exchange_account_id
    }

    pub fn currency_pair(&self) -> CurrencyPair {
        self.header().currency_pair
    }

    pub fn client_order_id(&self) -> ClientOrderId {
        self.header().client_order_id.clone()
    }

    pub fn side(&self) -> OrderSide {
        self.header().side
    }

    /// NOTE: Should be used only in cases when we sure that price specified
    pub fn price(&self) -> Price {
        self.header().price()
    }

    /// Price of order specified by exchange client before order creation.
    /// Price should be specified for `Limit` order and should not be specified for `Market` order.
    /// For other order types it depends on exchange requirements.
    pub fn source_price(&self) -> Option<Price> {
        self.header().source_price
    }

    pub fn amount(&self) -> Amount {
        self.header().amount
    }

    pub fn order_type(&self) -> OrderType {
        self.header().order_type
    }

    /// Lock order for read and provide copy mutable properties or check some conditions
    pub fn fn_ref<T: 'static>(&self, f: impl FnOnce(&OrderMut) -> T) -> T {
        f(self.inner.data.read().borrow())
    }

    /// Lock order for write and provide mutate state of order
    pub fn fn_mut<T: 'static>(&self, f: impl FnOnce(&mut OrderMut) -> T) -> T {
        f(self.inner.data.write().borrow_mut())
    }

    pub fn status(&self) -> OrderStatus {
        self.fn_ref(|x| x.status())
    }
    pub fn role(&self) -> Option<OrderRole> {
        self.fn_ref(|x| x.props.role)
    }
    pub fn is_finished(&self) -> bool {
        self.fn_ref(|x| x.is_finished())
    }
    pub fn was_cancellation_event_raised(&self) -> bool {
        self.fn_ref(|x| x.internal_props.was_cancellation_event_raised)
    }
    pub fn exchange_order_id(&self) -> Option<ExchangeOrderId> {
        self.fn_ref(|x| x.exchange_order_id())
    }
    pub fn order_ids(&self) -> (ClientOrderId, Option<ExchangeOrderId>) {
        let client_order_id = self.client_order_id();
        (client_order_id, self.fn_ref(|x| x.exchange_order_id()))
    }

    pub fn deep_clone(&self) -> OrderSnapshot {
        self.fn_ref(|order| OrderSnapshot {
            header: self.header().clone(),
            props: order.props.clone(),
            fills: order.fills.clone(),
            status_history: order.status_history.clone(),
            internal_props: order.internal_props.clone(),
            extension_data: order.extension_data.clone(),
        })
    }

    pub fn filled_amount(&self) -> Amount {
        self.fn_ref(|order| order.filled_amount())
    }
    pub fn get_fills(&self) -> (Vec<OrderFill>, Amount) {
        self.fn_ref(|order| (order.fills.fills.clone(), order.fills.filled_amount))
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct OrdersPool {
    pub cache_by_client_id: DashMap<ClientOrderId, OrderRef>,
    pub cache_by_exchange_id: DashMap<ExchangeOrderId, OrderRef>,
    pub not_finished: DashMap<ClientOrderId, OrderRef>,
}

impl OrdersPool {
    pub fn new() -> Arc<Self> {
        const ORDERS_INIT_CAPACITY: usize = 100;

        Arc::new(OrdersPool {
            cache_by_client_id: DashMap::with_capacity(ORDERS_INIT_CAPACITY),
            cache_by_exchange_id: DashMap::with_capacity(ORDERS_INIT_CAPACITY),
            not_finished: DashMap::with_capacity(ORDERS_INIT_CAPACITY),
        })
    }

    /// Built `OrderRef` by specified `OrderSnapshot` and Insert it in order pool.
    pub fn add_snapshot_initial(&self, snapshot: &OrderSnapshot) -> OrderRef {
        let client_order_id = snapshot.header.client_order_id.clone();

        let order_ref = OrderRef::from_snapshot(snapshot);
        let _ = self
            .cache_by_client_id
            .insert(client_order_id.clone(), order_ref.clone());
        let _ = self.not_finished.insert(client_order_id, order_ref.clone());

        order_ref
    }

    /// Create `OrderRef` by specified `OrderHeader` with default other properties and insert it in order pool.
    pub fn add_simple_initial(
        &self,
        header: &OrderHeader,
        init_time: DateTime,
        extension_data: Option<Box<dyn OrderInfoExtensionData>>,
    ) -> OrderRef {
        match self.cache_by_client_id.get(&header.client_order_id) {
            None => {
                let order = OrderRef {
                    inner: Arc::new(OrderRefData {
                        header: header.clone(),
                        data: RwLock::new(OrderMut {
                            props: OrderSimpleProps::from_init_time(init_time),
                            fills: Default::default(),
                            status_history: Default::default(),
                            internal_props: Default::default(),
                            extension_data,
                        }),
                    }),
                };

                let client_order_id = header.client_order_id.clone();
                let _ = self
                    .cache_by_client_id
                    .insert(client_order_id.clone(), order.clone());
                let _ = self.not_finished.insert(client_order_id, order.clone());

                order
            }
            Some(order) => {
                order.fn_mut(|x| x.props.init_time = init_time);
                order.clone()
            }
        }
    }
}
