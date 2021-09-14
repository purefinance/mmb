use crate::core::exchanges::common::{CurrencyPair, Price};
use crate::core::orders::order::OrderSide;

use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct DerivativePositionInfo {
    pub currency_pair: CurrencyPair,
    pub position: Decimal,
    pub side: Option<OrderSide>,
    pub average_entry_price: Price,
    pub liquidation_price: Price,
    pub leverage: Decimal,
}

impl DerivativePositionInfo {
    pub fn new(
        currency_pair: CurrencyPair,
        position: Decimal,
        side: Option<OrderSide>,
        average_entry_price: Price,
        liquidation_price: Price,
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
