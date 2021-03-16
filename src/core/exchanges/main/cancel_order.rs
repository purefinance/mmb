use anyhow::Result;
use futures::pin_mut;
use log::{error, info};
use tokio::sync::oneshot;

use crate::core::{
    exchanges::cancellation_token::CancellationToken,
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
    exchanges::common::RestRequestOutcome,
    orders::order::ClientOrderId,
    orders::order::ExchangeOrderId,
    orders::{fill::EventSourceType, order::OrderCancelling},
};

use super::{exchange::CancelOrderResult, exchange::Exchange, exchange::RequestResult};

impl Exchange {
    pub async fn cancel_order(
        &self,
        // TODO Here has to be common Order (or ORderRef) cause it's more natural way:
        // When user whant to cancle_order he already has that order data somewhere
        order: &OrderCancelling,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        let exchange_order_id = order.exchange_order_id.clone();
        let (tx, websocket_event_receiver) = oneshot::channel();

        self.order_cancellation_events
            .insert(exchange_order_id.clone(), (tx, None));

        let order_cancel_future = self.exchange_interaction.request_cancel_order(&order);
        let cancellation_token = cancellation_token.when_cancelled();

        pin_mut!(order_cancel_future);
        pin_mut!(cancellation_token);
        pin_mut!(websocket_event_receiver);

        tokio::select! {
            rest_request_outcome = &mut order_cancel_future => {
                let cancel_order_result = self.handle_cancel_order_response(&rest_request_outcome, &order);
                match cancel_order_result.outcome {
                    RequestResult::Error(_) => {
                        // TODO if ExchangeFeatures.Order.CreationResponseFromRestOnlyForError
                        return Some(cancel_order_result);
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
                return websocket_outcome.ok()
            }
        };
    }

    fn handle_cancel_order_response(
        &self,
        request_outcome: &Result<RestRequestOutcome>,
        order: &OrderCancelling,
    ) -> CancelOrderResult {
        info!(
            "Cancel response for {}, {:?}, {:?}",
            order.header.client_order_id, order.header.exchange_account_id, request_outcome
        );

        match request_outcome {
            Ok(request_outcome) => {
                if let Some(rest_error) = self.get_rest_error_order(request_outcome, &order.header)
                {
                    return CancelOrderResult::failed(rest_error, EventSourceType::Rest);
                }

                // TODO Parse request_outcome.content similarly to the handle_create_order_response
                CancelOrderResult::successed(
                    order.header.client_order_id.clone(),
                    EventSourceType::Rest,
                    None,
                )
            }
            Err(error) => {
                let exchange_error =
                    ExchangeError::new(ExchangeErrorType::SendError, error.to_string(), None);
                return CancelOrderResult::failed(exchange_error, EventSourceType::Rest);
            }
        }
    }

    pub(super) fn raise_order_cancelled(
        &self,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        source_type: EventSourceType,
    ) {
        if let Some((_, (tx, _))) = self.order_cancellation_events.remove(&exchange_order_id) {
            if let Err(error) = tx.send(CancelOrderResult::successed(
                client_order_id,
                source_type,
                None,
            )) {
                error!("Unable to send thru oneshot channel: {:?}", error);
            }
        }
    }
}
