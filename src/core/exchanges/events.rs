use crate::core::orders::{
    fill::OrderFill,
    order::{OrderEventType, OrderSnapshot},
};

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum AllowedEventSourceType {
    All,
    FallbackOnly,
    NonFallback,
}

#[derive(Debug, Clone)]
pub struct OrderEvent {
    pub order: OrderSnapshot,
    pub event_type: OrderEventType,
    pub order_fill: Option<OrderFill>,
}

impl OrderEvent {
    pub fn new(
        order: OrderSnapshot,
        event_type: OrderEventType,
        order_fill: Option<OrderFill>,
    ) -> Self {
        Self {
            order,
            event_type,
            order_fill,
        }
    }
}
