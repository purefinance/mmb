use crate::exchanges::events::AllowedEventSourceType;
use crate::orders::fill::EventSourceType;

pub mod handle_cancel_order_failed;
pub mod handle_cancel_order_succeeded;
pub mod handle_order_filled;
pub mod handle_trade;

pub(crate) fn should_ignore_event(
    allowed_event_source_type: AllowedEventSourceType,
    source_type: EventSourceType,
) -> bool {
    use AllowedEventSourceType::*;
    use EventSourceType::*;

    match allowed_event_source_type {
        FallbackOnly if source_type != RestFallback => true,
        NonFallback if source_type != Rest && source_type != WebSocket => true,
        _ => false,
    }
}
