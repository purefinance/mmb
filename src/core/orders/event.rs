use crate::core::orders::fill::OrderFill;
use crate::core::orders::order::{OrderEventType, OrderStatus};
use crate::core::orders::pool::OrderRef;

#[derive(Debug, Clone)]
pub struct OrderEvent {
    pub order: OrderRef,
    pub order_status: OrderStatus,
    pub order_fill: Option<OrderFill>,
    pub order_event_type: OrderEventType,
}
