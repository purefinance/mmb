use crate::{
    exchanges::common::ExchangeError, exchanges::common::ExchangeErrorType,
    exchanges::general::exchange::Exchange, orders::order::OrderInfo, orders::pool::OrderRef,
};
use anyhow::*;

impl Exchange {
    pub async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        if order.exchange_order_id().is_none()
            && !self
                .features
                .order_features
                .supports_get_order_info_by_client_order_id
        {
            let error_msg = "exchange_order_id should be set when exchange does not support getting order info by client order id"
                .to_owned();
            return Err(ExchangeError::new(
                ExchangeErrorType::Unknown,
                error_msg,
                None,
            ));
        }

        log::info!(
            "get_order_info response: {}, {:?} on {}",
            order.client_order_id(),
            order.exchange_order_id(),
            self.exchange_account_id
        );

        self.exchange_client.get_order_info(order).await
    }
}
