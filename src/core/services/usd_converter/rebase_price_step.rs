use std::sync::Arc;

use crate::core::exchanges::{
    common::ExchangeId, general::currency_pair_metadata::CurrencyPairMetadata,
};

#[derive(Clone, Debug)]
pub enum RebaseDirection {
    ToQuote,
    ToBase,
}

#[derive(Clone, Debug)]
pub struct RebasePriceStep {
    pub exchange_id: ExchangeId,
    pub currency_pair_metadata: Arc<CurrencyPairMetadata>,
    pub direction: RebaseDirection,
}

impl RebasePriceStep {
    pub fn new(
        exchange_id: ExchangeId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        direction: RebaseDirection,
    ) -> Self {
        Self {
            exchange_id,
            currency_pair_metadata,
            direction,
        }
    }
}
