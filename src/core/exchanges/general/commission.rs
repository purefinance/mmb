use crate::core::orders::order::OrderRole;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub type Percent = Decimal;

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct CommissionForType {
    pub fee: Percent,
    pub referral_reward: Percent,
}

impl CommissionForType {
    pub fn new(fee: Percent, referral_reward: Percent) -> Self {
        Self {
            fee,
            referral_reward,
        }
    }
}

pub fn percent_to_rate(percent_value: Percent) -> Decimal {
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
