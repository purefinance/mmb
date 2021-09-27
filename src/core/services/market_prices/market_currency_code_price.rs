use crate::core::exchanges::common::{CurrencyCode, Price};

#[derive(PartialEq, Eq, Clone)]
pub struct MarketCurrencyCodePrice {
    pub symbol: CurrencyCode,
    pub price_usd: Option<Price>,
}

impl MarketCurrencyCodePrice {
    pub fn new(symbol: CurrencyCode, price_usd: Option<Price>) -> Self {
        Self { symbol, price_usd }
    }
}
