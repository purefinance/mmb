use crate::core::{
    exchanges::common::ExchangeError, exchanges::common::ExchangeErrorType,
    exchanges::general::exchange::Exchange, orders::order::OrderInfo, orders::pool::OrderRef,
};
use anyhow::*;

impl Exchange {
    pub async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        if order.exchange_order_id().is_none()
            && self.features.allows_to_get_order_info_by_client_order_id
        {
            let error_msg = "exchange_order_id should be set when exchange does not support getting order info by client order id"
                .to_owned();
            return Err(ExchangeError::new(
                ExchangeErrorType::Unknown,
                error_msg,
                None,
            ));
        }

        self.get_order_info_core(&order).await
    }

    async fn get_order_info_core(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        log::info!(
            "get_order_info response: {}, {:?} on {}",
            order.client_order_id(),
            order.exchange_order_id(),
            self.exchange_account_id
        );
        let request_outcome = self.exchange_client.request_order_info(order).await;

        match request_outcome {
            Ok(request_outcome) => {
                let order_header = order.fn_ref(|order| order.header.clone());
                if let Some(exchange_error) =
                    self.get_rest_error_order(&request_outcome, &order_header)
                {
                    return Err(exchange_error);
                }

                let unified_order_info = self.exchange_client.parse_order_info(&request_outcome);

                match unified_order_info {
                    Ok(order_info) => Ok(order_info),
                    Err(error) => Err(ExchangeError::new(
                        ExchangeErrorType::OrderNotFound,
                        error.to_string(),
                        None,
                    )),
                }
            }
            Err(error) => Err(ExchangeError::new(
                ExchangeErrorType::Unknown,
                error.to_string(),
                None,
            )),
        }
    }
}
