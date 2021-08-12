use std::collections::HashMap;

use crate::core::balance_manager::approved_part::ApprovedPart;
use crate::core::exchanges::common::Amount;
use crate::core::exchanges::common::CurrencyCode;
use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::orders::order::OrderSide;
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

use rust_decimal::Decimal;

pub(crate) struct BalanceReservation {
    pub configuration_descriptor: ConfigurationDescriptor,
    pub exchange_account_id: ExchangeAccountId,
    // public Symbol Symbol { get; set; }
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
    pub approved_parts: HashMap<String, ApprovedPart>,
}
