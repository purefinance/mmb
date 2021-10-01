use crate::core::{
    exchanges::common::{Amount, CurrencyCode},
    lifecycle::cancellation_token::CancellationToken,
};

use anyhow::Result;

pub struct PriceSourceService {}

impl PriceSourceService {
    pub async fn convert_amount(
        &self,
        _from_currency_code: &CurrencyCode,
        _to_currency_code: &CurrencyCode,
        _src_amount: Amount,
        _cancellation_token: CancellationToken,
    ) -> Result<Option<Amount>> {
        //TODO: should be implemented
        Ok(None)
    }
}
