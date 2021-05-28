use crate::core::{
    exchanges::common::ExchangeError, orders::fill::EventSourceType, orders::order::ExchangeOrderId,
};

use super::exchange::Exchange;

impl Exchange {
    // TODO implement
    pub(crate) fn handle_cancel_order_failed(
        &self,
        _exchange_order_id: Option<ExchangeOrderId>,
        _error: ExchangeError,
        _event_source_type: EventSourceType,
    ) {
    }
}
