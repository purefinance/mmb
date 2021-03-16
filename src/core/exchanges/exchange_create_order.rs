use anyhow::Result;
use futures::pin_mut;
use log::info;
use tokio::sync::oneshot;

use crate::core::orders::{fill::EventSourceType, order::OrderCreating};

use super::{
    cancellation_token::CancellationToken, common::ExchangeError, common::ExchangeErrorType,
    common::RestRequestOutcome, exchange::CreateOrderResult, exchange::Exchange,
    exchange::RequestResult,
};

impl Exchange {
    pub async fn create_order(
        &self,
        order: &OrderCreating,
        cancellation_token: CancellationToken,
    ) -> Option<CreateOrderResult> {
        let client_order_id = order.header.client_order_id.clone();
        let (tx, websocket_event_receiver) = oneshot::channel();

        self.order_creation_events
            .insert(client_order_id.clone(), (tx, None));

        let order_create_future = self.exchange_interaction.create_order(&order);
        let cancellation_token = cancellation_token.when_cancelled();

        pin_mut!(order_create_future);
        pin_mut!(cancellation_token);
        pin_mut!(websocket_event_receiver);

        tokio::select! {
            rest_request_outcome = &mut order_create_future => {
                let create_order_result = self.handle_create_order_response(&rest_request_outcome, &order);
                match create_order_result.outcome {
                    RequestResult::Error(_) => {
                        // TODO if ExchangeFeatures.Order.CreationResponseFromRestOnlyForError
                        return Some(create_order_result);
                    }

                    RequestResult::Success(_) => {
                        tokio::select! {
                            websocket_outcome = &mut websocket_event_receiver => {
                                return websocket_outcome.ok()
                            }

                            _ = &mut cancellation_token => {
                                return None;
                            }

                        }
                    }
                }
            }

            _ = &mut cancellation_token => {
                return None;
            }

            websocket_outcome = &mut websocket_event_receiver => {
                return websocket_outcome.ok();
            }

        };
    }

    fn handle_create_order_response(
        &self,
        request_outcome: &Result<RestRequestOutcome>,
        order: &OrderCreating,
    ) -> CreateOrderResult {
        info!(
            "Create response for {}, {:?}, {:?}",
            // TODO other order_headers_field
            order.header.client_order_id,
            order.header.exchange_account_id,
            request_outcome
        );

        match request_outcome {
            Ok(request_outcome) => {
                if let Some(rest_error) = self.get_rest_error_order(request_outcome, &order.header)
                {
                    return CreateOrderResult::failed(rest_error, EventSourceType::Rest);
                }

                match self.exchange_interaction.get_order_id(&request_outcome) {
                    Ok(created_order_id) => {
                        CreateOrderResult::successed(created_order_id, EventSourceType::Rest)
                    }
                    Err(error) => {
                        let exchange_error = ExchangeError::new(
                            ExchangeErrorType::ParsingError,
                            error.to_string(),
                            None,
                        );
                        CreateOrderResult::failed(exchange_error, EventSourceType::Rest)
                    }
                }
            }
            Err(error) => {
                let exchange_error =
                    ExchangeError::new(ExchangeErrorType::SendError, error.to_string(), None);
                return CreateOrderResult::failed(exchange_error, EventSourceType::Rest);
            }
        }
    }
}
