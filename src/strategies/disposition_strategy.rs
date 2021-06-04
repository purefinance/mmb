use std::sync::Arc;

use anyhow::Result;

use crate::core::disposition_execution::{PriceSlot, TradingContext};
use crate::core::exchanges::cancellation_token::CancellationToken;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::orders::order::OrderSnapshot;

pub struct DispositionStrategy {}

impl DispositionStrategy {
    pub fn new() -> Arc<Self> {
        Arc::new(DispositionStrategy {})
    }

    pub fn estimate(&self) -> Result<TradingContext> {
        todo!("need implementation")
    }

    pub fn handle_order_fill(
        &self,
        _cloned_order: &Arc<OrderSnapshot>,
        _price_slot: &PriceSlot,
        _target_eai: &ExchangeAccountId,
        _cancellation_token: CancellationToken,
    ) -> Result<()> {
        // TODO save order fill info in Database
        Ok(())
    }
}
