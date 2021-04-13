use crate::core::orders::fill::OrderFill;
use crate::core::orders::order::OrderEventType;
use crate::core::orders::pool::OrderRef;

#[derive(Debug, Clone)]
pub struct OrderEvent {
    pub order: OrderRef,
    pub event_type: OrderEventType,
    pub order_fill: Option<OrderFill>,
}

impl OrderEvent {
    pub fn new(order: OrderRef, event_type: OrderEventType, order_fill: Option<OrderFill>) -> Self {
        Self {
            order,
            event_type,
            order_fill,
        }
    }
}
