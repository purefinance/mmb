use anyhow::Result;
use log::{error, info};
use tokio::sync::oneshot;

use crate::core::{
    exchanges::cancellation_token::CancellationToken,
    exchanges::common::Amount,
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
    exchanges::common::RestRequestOutcome,
    exchanges::general::exchange::Exchange,
    exchanges::general::exchange::RequestResult,
    orders::order::ClientOrderId,
    orders::order::ExchangeOrderId,
    orders::{fill::EventSourceType, order::OrderCancelling},
};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CancelOrderResult {
    pub outcome: RequestResult<ClientOrderId>,
    pub source_type: EventSourceType,
    // TODO Use it in the future
    pub filled_amount: Option<Amount>,
}

impl CancelOrderResult {
    pub fn successed(
        client_order_id: ClientOrderId,
        source_type: EventSourceType,
        filled_amount: Option<Amount>,
    ) -> Self {
        CancelOrderResult {
            outcome: RequestResult::Success(client_order_id),
            source_type,
            filled_amount,
        }
    }

    pub fn failed(error: ExchangeError, source_type: EventSourceType) -> Self {
        CancelOrderResult {
            outcome: RequestResult::Error(error),
            source_type,
            filled_amount: None,
        }
    }
}

impl Exchange {
    pub async fn cancel_order(
        &self,
        // TODO Here has to be common Order (or OrderRef) cause it's more natural way:
        // When user want to cancel_order he already has that order data somewhere
        order: &OrderCancelling,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        let exchange_order_id = order.exchange_order_id.clone();
        let (tx, mut websocket_event_receiver) = oneshot::channel();

        self.order_cancellation_events
            .insert(exchange_order_id.clone(), (tx, None));

        let order_cancel_future = self.exchange_client.request_cancel_order(&order);

        tokio::select! {
            rest_request_outcome = order_cancel_future => {
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
                            _ = cancellation_token.when_cancelled() => {
                                return None;
                            }
                        }
                    }
                }
            }
            _ = cancellation_token.when_cancelled() => {
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

    pub(crate) fn raise_order_cancelled(
        &self,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        source_type: EventSourceType,
    ) -> Result<()> {
        let filled_amount = None;
        match self.order_cancellation_events.remove(&exchange_order_id) {
            Some((_, (tx, _))) => {
                if let Err(error) = tx.send(CancelOrderResult::successed(
                    client_order_id,
                    source_type,
                    filled_amount,
                )) {
                    error!(
                        "raise_order_cancelled failed: unable to send thru oneshot channel: {:?}",
                        error
                    );
                }

                Ok(())
            }
            None => self.handle_cancel_order_succeeded(
                &client_order_id,
                &exchange_order_id,
                filled_amount,
                source_type,
            ),
        }
    }
}
