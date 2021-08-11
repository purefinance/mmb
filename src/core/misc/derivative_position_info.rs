use crate::core::exchanges::common::CurrencyPair;
use crate::core::orders::order::OrderSide;

use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct DerivativePositionInfo {
    pub currency_pair: CurrencyPair,
    pub position: Decimal,
    pub side: OrderSide,
    pub average_entry_price: Decimal,
    pub liquidation_price: Decimal,
    pub leverage: Decimal,
}

impl DerivativePositionInfo {
    pub fn new(
        currency_pair: CurrencyPair,
        position: Decimal,
        side: OrderSide,
        average_entry_price: Decimal,
        liquidation_price: Decimal,
        leverage: Decimal,
    ) -> DerivativePositionInfo {
        DerivativePositionInfo {
            currency_pair,
            position,
            side,
            average_entry_price,
            liquidation_price,
            leverage,
        }
    }
}
