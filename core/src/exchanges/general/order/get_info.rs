use crate::exchanges::general::exchange::Exchange;
use crate::exchanges::traits::ExchangeError;
use anyhow::*;
use domain::market::ExchangeErrorType;
use domain::order::pool::OrderRef;
use domain::order::snapshot::OrderInfo;

impl Exchange {
    pub async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        let (client_order_id, exchange_order_id) = order.order_ids();
        if exchange_order_id.is_none()
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
            "get_order_info response: {client_order_id}, {exchange_order_id:?} on {}",
            self.exchange_account_id
        );

        self.exchange_client.get_order_info(order).await
    }
}
