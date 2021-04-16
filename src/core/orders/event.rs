use crate::core::orders::fill::OrderFill;
use crate::core::orders::order::OrderEventType;
use crate::core::orders::pool::OrderRef;

use super::order::OrderStatus;

#[derive(Debug, Clone)]
pub struct OrderEvent {
    pub order: OrderRef,
    pub status: OrderStatus,
    pub event_type: OrderEventType,
    pub order_fill: Option<OrderFill>,
}

impl OrderEvent {
    pub fn new(
        order: OrderRef,
        status: OrderStatus,
        event_type: OrderEventType,
        order_fill: Option<OrderFill>,
    ) -> Self {
        Self {
            order,
            status,
            event_type,
            order_fill,
        }
    }
}
