use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::{
    exchanges::common::{Amount, CurrencyCode},
    lifecycle::cancellation_token::CancellationToken,
};

use super::{
    denominator_usd_converter::DenominatorUsdConverter, price_source_service::PriceSourceService,
    usd_denominator::UsdDenominator,
};

pub struct UsdConverter {
    price_source_service: PriceSourceService,
    usd_currency_code: CurrencyCode,
    denominator_usd_converter: DenominatorUsdConverter,
}

impl UsdConverter {
    pub fn new(
        currencies: &Vec<CurrencyCode>,
        price_source_service: PriceSourceService,
        usd_denominator: Arc<Mutex<UsdDenominator>>,
    ) -> Self {
        Self {
            price_source_service,
            usd_currency_code: currencies
                .iter()
                .find(|&x| x == &CurrencyCode::from("USDT") || x == &CurrencyCode::from("USD"))
                .cloned()
                .unwrap_or(CurrencyCode::from("USD")),
            denominator_usd_converter: DenominatorUsdConverter::new(usd_denominator),
        }
    }

    pub async fn convert_amount(
        &self,
        from_currency_code: &CurrencyCode,
        src_amount: Amount,
        cancellation_token: CancellationToken,
    ) -> Option<Amount> {
        if from_currency_code == &self.usd_currency_code {
            return Some(src_amount);
        }

        match self
            .price_source_service
            .convert_amount(
                from_currency_code,
                &self.usd_currency_code,
                src_amount,
                cancellation_token,
            )
            .await
        {
            Ok(usd_amount) => {
                if usd_amount.is_some() {
                    return usd_amount;
                }
            }
            Err(error) => log::warn!(
                "Failed to calculate price {} -> {}: {:?}",
                from_currency_code,
                self.usd_currency_code,
                error
            ),
        }

        log::warn!("Can't calculate USD price using PriceSourceService => trying to use UsdDenominator ({})", from_currency_code);

        self.denominator_usd_converter
            .calculate_using_denominator(from_currency_code, src_amount)
            .await
    }
}
