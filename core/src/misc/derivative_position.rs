use crate::exchanges::common::{CurrencyPair, Price};
use crate::orders::order::OrderSide;

use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct DerivativePosition {
    pub currency_pair: CurrencyPair,
    pub position: Decimal,
    pub side: Option<OrderSide>,
    pub average_entry_price: Price,
    pub liquidation_price: Price,
    pub leverage: Decimal,
}

impl DerivativePosition {
    pub fn new(
        currency_pair: CurrencyPair,
        position: Decimal,
        side: Option<OrderSide>,
        average_entry_price: Price,
        liquidation_price: Price,
        leverage: Decimal,
    ) -> DerivativePosition {
        DerivativePosition {
            currency_pair,
            position,
            side,
            average_entry_price,
            liquidation_price,
            leverage,
        }
    }
}
