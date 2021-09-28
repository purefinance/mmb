use crate::core::exchanges::{
    common::ExchangeId, general::currency_pair_metadata::CurrencyPairMetadata,
};

pub struct RebasePriceStep {
    pub exchange_id: ExchangeId,
    pub currency_pair_metadata: CurrencyPairMetadata,
    pub from_base_to_quote_currency: bool,
}

impl RebasePriceStep {
    pub fn new(
        exchange_id: ExchangeId,
        currency_pair_metadata: CurrencyPairMetadata,
        from_base_to_quote_currency: bool,
    ) -> Self {
        Self {
            exchange_id,
            currency_pair_metadata,
            from_base_to_quote_currency,
        }
    }
}
