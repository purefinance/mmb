use crate::core::exchanges::common::{Amount, CurrencyPair};
use crate::core::misc::derivative_position_info::DerivativePositionInfo;
use crate::core::orders::order::OrderSide;

use rust_decimal::Decimal;

pub struct ActivePosition {
    // pub id: usize,
    pub currency_pair: CurrencyPair,
    pub order_side: OrderSide,
    // pub base: Decimal,
    pub amount: Amount,
    pub leverage: Decimal,
    pub average_entry_price: Decimal,
    pub liquidation_price: Decimal,
}

impl ActivePosition {
    pub fn new(positionInfo: &DerivativePositionInfo) -> ActivePosition {
        ActivePosition {
            currency_pair: positionInfo.currency_pair.clone(),
            order_side: positionInfo.side,
            amount: positionInfo.position,
            leverage: positionInfo.leverage,
            average_entry_price: positionInfo.average_entry_price,
            liquidation_price: positionInfo.liquidation_price,
        }
    }
}
