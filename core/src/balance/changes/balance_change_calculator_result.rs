use domain::market::{CurrencyCode, ExchangeId};
use domain::order::snapshot::{Amount, Price};
use mockall_double::double;

use crate::misc::service_value_tree::ServiceValueTree;
#[double]
use crate::services::usd_convertion::usd_converter::UsdConverter;

use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
#[derive(Debug)]
pub(crate) struct BalanceChangesCalculatorResult {
    balance_changes: ServiceValueTree,
    currency_code: CurrencyCode,
    price: Price,
    pub exchange_id: ExchangeId,
}

impl BalanceChangesCalculatorResult {
    pub fn new(
        balance_changes: ServiceValueTree,
        currency_code: CurrencyCode,
        price: Price,
        exchange_id: ExchangeId,
    ) -> Self {
        Self {
            balance_changes,
            currency_code,
            price,
            exchange_id,
        }
    }

    pub async fn calculate_usd_change(
        &self,
        currency_code: CurrencyCode,
        balance_change: Amount,
        usd_converter: &UsdConverter,
        cancellation_token: CancellationToken,
    ) -> Amount {
        match self.currency_code.as_str().starts_with("usd") {
            true => match currency_code.as_str().starts_with("usd") {
                false => balance_change * self.price,
                true => balance_change,
            },
            false => usd_converter
                .convert_amount(currency_code, balance_change, cancellation_token)
                .await
                .with_expect(|| format!("Failed to convert from {} to USD", currency_code)),
        }
    }

    pub fn get_changes(&self) -> &ServiceValueTree {
        &self.balance_changes
    }
}
