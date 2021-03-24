use crate::core::{
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
    exchanges::general::exchange::Exchange,
    orders::order::{OrderInfo, OrderSnapshot},
};
use anyhow::*;
use log::info;

impl Exchange {
    pub async fn get_order_info(&self, order: &OrderSnapshot) -> Result<OrderInfo, ExchangeError> {
        if order.props.exchange_order_id.is_none()
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

        self.get_order_info_core(order).await
    }

    async fn get_order_info_core(&self, order: &OrderSnapshot) -> Result<OrderInfo, ExchangeError> {
        info!(
            "get_order_info response: {}, {:?} on {}",
            order.header.client_order_id, order.props.exchange_order_id, self.exchange_account_id
        );
        let request_outcome = self.exchange_client.request_order_info(order).await;

        match request_outcome {
            Ok(request_outcome) => {
                if let Some(exchange_error) =
                    self.get_rest_error_order(&request_outcome, &order.header)
                {
                    return Err(exchange_error);
                }

                let unified_order_info = self.exchange_client.parse_order_info(&request_outcome);

                match unified_order_info {
                    Ok(_) => {}
                    Err(error) => {
                        return Err(ExchangeError::new(
                            ExchangeErrorType::OrderNotFound,
                            error.to_string(),
                            None,
                        ))
                    }
                }
            }
            Err(error) => {
                ExchangeError::new(ExchangeErrorType::Unknown, error.to_string(), None);
            }
        }

        // FIXME delete
        Err(ExchangeError::new(
            ExchangeErrorType::Unknown,
            "test".to_owned(),
            None,
        ))
    }
}
