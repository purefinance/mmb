use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::exchanges::common::{Amount, CurrencyCode};

use super::usd_denominator::UsdDenominator;

pub struct DenominatorUsdConverter {
    usd_denominator: Arc<Mutex<UsdDenominator>>,
}

impl DenominatorUsdConverter {
    pub fn new(usd_denominator: Arc<Mutex<UsdDenominator>>) -> Self {
        Self { usd_denominator }
    }

    pub(super) async fn calculate_using_denominator(
        &self,
        from_currency_code: &CurrencyCode,
        src_amount: Amount,
    ) -> Option<Amount> {
        self.usd_denominator
            .lock()
            .await
            .currency_to_usd(from_currency_code, src_amount)
    }
}
