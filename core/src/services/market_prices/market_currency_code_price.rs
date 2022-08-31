use domain::market::CurrencyCode;
use domain::order::snapshot::Price;

#[derive(PartialEq, Eq, Clone)]
pub struct MarketCurrencyCodePrice {
    pub currency_code: CurrencyCode,
    pub price_usd: Option<Price>,
}

impl MarketCurrencyCodePrice {
    pub fn new(currency_code: CurrencyCode, price_usd: Option<Price>) -> Self {
        Self {
            currency_code,
            price_usd,
        }
    }
}
