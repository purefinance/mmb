use crate::core::orders::order::OrderRole;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct CommissionForType {
    pub fee: Decimal,
    pub referral_reward: Decimal,
}

impl CommissionForType {
    pub fn new(fee: Decimal, referral_reward: Decimal) -> Self {
        Self {
            fee,
            referral_reward,
        }
    }
}

pub fn percent_to_rate(percent_value: Decimal) -> Decimal {
    let proportion_multiplier = dec!(0.01);
    percent_value * proportion_multiplier
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct Commission {
    pub maker: CommissionForType,
    pub taker: CommissionForType,
}

impl Commission {
    pub fn new(maker: CommissionForType, taker: CommissionForType) -> Self {
        Self { maker, taker }
    }

    pub fn get_commission(&self, order_role: OrderRole) -> CommissionForType {
        match order_role {
            OrderRole::Maker => self.maker.clone(),
            OrderRole::Taker => self.taker.clone(),
        }
    }
}
