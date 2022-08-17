use std::collections::HashMap;
use std::sync::Arc;

use crate::balance::manager::approved_part::ApprovedPart;
use crate::exchanges::common::Amount;
use crate::exchanges::common::CurrencyCode;
use crate::exchanges::common::ExchangeAccountId;
use crate::exchanges::common::Price;
use crate::exchanges::general::symbol::Symbol;
use crate::orders::order::ClientOrderId;
use crate::orders::order::OrderSide;
use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use serde::Serialize;

use anyhow::{bail, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Clone, Debug, Serialize)]
pub struct BalanceReservation {
    pub configuration_descriptor: ConfigurationDescriptor,
    pub exchange_account_id: ExchangeAccountId,
    pub symbol: Arc<Symbol>,
    pub order_side: OrderSide,
    pub price: Price,
    pub amount: Amount,
    pub taken_free_amount: Amount,
    pub cost: Decimal,

    /// CurrencyCode in which we take away amount
    pub reservation_currency_code: CurrencyCode,
    pub unreserved_amount: Amount,

    /// Not approved amount in AmountCurrencyCode
    pub not_approved_amount: Amount,
    pub approved_parts: HashMap<ClientOrderId, ApprovedPart>,
}

impl BalanceReservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        order_side: OrderSide,
        price: Price,
        amount: Amount,
        taken_free_amount: Amount,
        cost: Decimal,
        reservation_currency_code: CurrencyCode,
    ) -> Self {
        Self {
            configuration_descriptor,
            exchange_account_id,
            symbol,
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

    pub(crate) fn get_proportional_cost_amount(&self, amount: Amount) -> Result<Decimal> {
        if self.amount.is_zero() {
            if !amount.is_zero() {
                bail!("Trying to receive a {} proportion out of zero", amount)
            }
            return Ok(dec!(0));
        }

        Ok(self.cost * amount / self.amount)
    }

    pub fn is_amount_within_symbol_margin_error(&self, amount: Amount) -> bool {
        amount.abs() <= self.symbol.get_amount_tick() * dec!(0.01)
    }

    pub(crate) fn convert_in_reservation_currency(
        &self,
        amount_in_current_currency: Amount,
    ) -> Amount {
        self.symbol.convert_amount_from_amount_currency_code(
            self.reservation_currency_code,
            amount_in_current_currency,
            self.price,
        )
    }
}
