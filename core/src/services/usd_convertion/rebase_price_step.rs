use std::sync::Arc;

use crate::exchanges::{common::ExchangeId, general::symbol::Symbol};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RebaseDirection {
    ToQuote,
    ToBase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebasePriceStep {
    pub exchange_id: ExchangeId,
    pub symbol: Arc<Symbol>,
    pub direction: RebaseDirection,
}

impl RebasePriceStep {
    pub fn new(exchange_id: ExchangeId, symbol: Arc<Symbol>, direction: RebaseDirection) -> Self {
        Self {
            exchange_id,
            symbol,
            direction,
        }
    }
}
