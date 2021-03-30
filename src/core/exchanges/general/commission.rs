use anyhow::{bail, Result};

use crate::core::{exchanges::common::Price, orders::order::OrderRole};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CommissionForType {
    pub fee: Price,
    pub referral_reward: Price,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Commission {
    pub maker: CommissionForType,
    pub taker: CommissionForType,
}

impl Commission {
    pub fn get_commission(&self, order_role: Option<OrderRole>) -> Result<CommissionForType> {
        match order_role {
            Some(order_role) => match order_role {
                OrderRole::Maker => return Ok(self.maker.clone()),
                OrderRole::Taker => return Ok(self.taker.clone()),
            },
            None => bail!("Cannot get fee because there are no order_role"),
        }
    }
}
