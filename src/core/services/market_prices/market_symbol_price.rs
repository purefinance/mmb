use rust_decimal::Decimal;

use crate::core::exchanges::common::CurrencyCode;

#[derive(PartialEq, Eq, Clone)]
pub struct MarketSymbolPrice {
    pub symbol: CurrencyCode,
    pub price_usd: Option<Decimal>,
}

impl MarketSymbolPrice {
    pub fn new(symbol: CurrencyCode, price_usd: Option<Decimal>) -> Self {
        Self { symbol, price_usd }
    }
}
