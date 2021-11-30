use std::sync::Arc;

use crate::core::exchanges::{
    common::ExchangeId, general::currency_pair_metadata::CurrencyPairMetadata,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RebaseDirection {
    ToQuote,
    ToBase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebasePriceStep {
    pub exchange_id: ExchangeId,
    pub symbol: Arc<CurrencyPairMetadata>,
    pub direction: RebaseDirection,
}

impl RebasePriceStep {
    pub fn new(
        exchange_id: ExchangeId,
        symbol: Arc<CurrencyPairMetadata>,
        direction: RebaseDirection,
    ) -> Self {
        Self {
            exchange_id,
            symbol,
            direction,
        }
    }
}
