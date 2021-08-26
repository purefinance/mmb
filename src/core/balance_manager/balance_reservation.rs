use std::collections::HashMap;
use std::sync::Arc;

use crate::core::balance_manager::approved_part::ApprovedPart;
use crate::core::exchanges::common::Amount;
use crate::core::exchanges::common::CurrencyCode;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::orders::order::ClientOrderId;
use crate::core::orders::order::OrderSide;
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

use anyhow::{bail, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Clone, Debug)]
pub(crate) struct BalanceReservation {
    pub configuration_descriptor: ConfigurationDescriptor,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair_metadata: Arc<CurrencyPairMetadata>,
    pub order_side: OrderSide,
    pub price: Decimal,
    pub amount: Amount,
    pub taken_free_amount: Decimal,
    pub cost: Decimal,

    /// CurrencyCode in which we take away amount
    pub reservation_currency_code: CurrencyCode, // maybe it should be string
    pub unreserved_amount: Decimal,

    /// Not approved amount in AmountCurrencyCode
    pub not_approved_amount: Decimal,
    pub approved_parts: HashMap<ClientOrderId, ApprovedPart>,
}

impl BalanceReservation {
    pub fn new(
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        order_side: OrderSide,
        price: Decimal,
        amount: Amount,
        taken_free_amount: Decimal,
        cost: Decimal,
        reservation_currency_code: CurrencyCode,
    ) -> Self {
        Self {
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata,
            order_side,
            price,
            amount,
            taken_free_amount,
            cost,
            reservation_currency_code,
            unreserved_amount: dec!(0),
            not_approved_amount: amount,
            approved_parts: HashMap::new(),
        }
    }

    pub(crate) fn get_proportional_cost_amount(&self, amount: Decimal) -> Result<Decimal> {
        if self.amount == dec!(0) {
            if amount != dec!(0) {
                bail!("Trying to receive a {} proportion out of zero", amount)
            }
            return Ok(dec!(0));
        }

        Ok(self.cost * amount / self.amount)
    }

    pub fn is_amount_within_symbol_margin_error(&self, amount: Decimal) -> bool {
        amount.abs()
            <= self
                .currency_pair_metadata
                .get_amount_tick()
                .expect("failed to get amount tick")
                * dec!(0.01)
    }

    pub(crate) fn convert_in_reservation_currency(
        &self,
        amount_in_current_currency: Decimal,
    ) -> Result<Decimal> {
        self.currency_pair_metadata
            .convert_amount_from_amount_currency_code(
                &self.reservation_currency_code,
                amount_in_current_currency,
                self.price,
            )
    }
}
