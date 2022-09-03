use mmb_domain::market::CurrencyCode;

use super::rebase_price_step::RebasePriceStep;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceSourceChain {
    pub start_currency_code: CurrencyCode,
    pub end_currency_code: CurrencyCode,
    pub rebase_price_steps: Vec<RebasePriceStep>,
}

impl PriceSourceChain {
    pub fn new(
        start_currency_code: CurrencyCode,
        end_currency_code: CurrencyCode,
        rebase_price_steps: Vec<RebasePriceStep>,
    ) -> Self {
        Self {
            start_currency_code,
            end_currency_code,
            rebase_price_steps,
        }
    }
}
