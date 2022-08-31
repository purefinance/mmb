use domain::order::snapshot::Amount;
use std::sync::Arc;

use domain::market::CurrencyCode;

use super::usd_denominator::UsdDenominator;

pub struct DenominatorUsdConverter {
    usd_denominator: Arc<UsdDenominator>,
}

impl DenominatorUsdConverter {
    pub fn new(usd_denominator: Arc<UsdDenominator>) -> Self {
        Self { usd_denominator }
    }

    pub(super) async fn calculate_using_denominator(
        &self,
        from_currency_code: CurrencyCode,
        src_amount: Amount,
    ) -> Option<Amount> {
        self.usd_denominator
            .currency_to_usd(from_currency_code, src_amount)
    }
}
