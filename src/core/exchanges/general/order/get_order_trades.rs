use anyhow::Result;

use crate::core::{exchanges::general::exchange::Exchange, orders::pool::OrderRef};

struct OrderTrade {}

impl Exchange {
    async fn get_order_trades(&self, order: OrderRef) -> Result<Vec<OrderTrade>> {
        match self.features.rest_fills_features.fills_type {
            crate::core::exchanges::general::features::RestFillsType::None => todo!(),
            crate::core::exchanges::general::features::RestFillsType::OrderTrades => todo!(),
            crate::core::exchanges::general::features::RestFillsType::MyTrades => todo!(),
            crate::core::exchanges::general::features::RestFillsType::GetOrderInfo => todo!(),
        }
        todo!()
    }
}
