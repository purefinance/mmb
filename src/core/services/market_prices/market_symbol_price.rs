use std::sync::Arc;

use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;

use rust_decimal::Decimal;

pub(crate) struct MarketSymbolPrice {
    pub(crate) symbol: String,
    pub(crate) price_usd: Option<Decimal>,
}

impl MarketSymbolPrice {
    pub fn new(symbol: String, price_usd: Option<Decimal>) -> Self {
        Self { symbol, price_usd }
    }
}
