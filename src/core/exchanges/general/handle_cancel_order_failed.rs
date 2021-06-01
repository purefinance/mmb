use crate::core::{
    exchanges::common::ExchangeError, orders::fill::EventSourceType, orders::order::ExchangeOrderId,
};

use super::exchange::Exchange;
use anyhow::Result;

impl Exchange {
    // TODO implement
    pub(crate) fn handle_cancel_order_failed(
        &self,
        _exchange_order_id: ExchangeOrderId,
        _error: ExchangeError,
        _event_source_type: EventSourceType,
    ) -> Result<()> {
        Ok(())
    }
}
