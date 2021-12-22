use anyhow::{anyhow, Result};
use chrono::Utc;
use futures::future::join_all;
use itertools::Itertools;
use mmb_utils::cancellation_token::CancellationToken;
use tokio::sync::oneshot;

use crate::core::{
    exchanges::common::Amount,
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
    exchanges::common::RestRequestOutcome,
    exchanges::general::exchange::Exchange,
    exchanges::general::exchange::RequestResult,
    orders::order::ClientOrderId,
    orders::order::ExchangeOrderId,
    orders::order::OrderInfo,
    orders::order::OrderStatus,
    orders::pool::OrderRef,
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
    pub async fn start_cancel_order(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Result<Option<CancelOrderResult>> {
        match order.status() {
            OrderStatus::Canceled => {
                log::info!(
                    "This order {} {:?} are already canceled",
                    order.client_order_id(),
                    order.exchange_order_id()
                );

                Ok(None)
            }
            OrderStatus::Completed => {
                log::info!(
                    "This order {} {:?} are already completed",
                    order.client_order_id(),
                    order.exchange_order_id()
                );

                Ok(None)
            }
            _ => {
                order.fn_mut(|order| order.set_status(OrderStatus::Canceling, Utc::now()));

                log::info!(
                    "Submitting order cancellation {} {:?} on {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id
                );

                let order_to_cancel = order
                    .to_order_cancelling()
                    .ok_or(anyhow!("Unable to convert order to order_to_cancel"))?;
                let order_cancellation_outcome = self
                    .cancel_order(&order_to_cancel, cancellation_token)
                    .await?;

                log::info!(
                    "Submitted order cancellation {} {:?} on {}: {:?}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                    order_cancellation_outcome
                );

                Ok(order_cancellation_outcome)
            }
        }
    }

    pub async fn cancel_order(
        &self,
        order: &OrderCancelling,
        cancellation_token: CancellationToken,
    ) -> Result<Option<CancelOrderResult>> {
        let order_cancellation_outcome = self.cancel_order_core(order, cancellation_token).await;

        // Option is returning when cancel_order_core is stopped by CancellationToken
        // So approptiate Handler was already called in a fallback
        if let Some(ref cancel_outcome) = order_cancellation_outcome {
            match &cancel_outcome.outcome {
                RequestResult::Success(client_order_id) => self.handle_cancel_order_succeeded(
                    Some(&client_order_id),
                    &order.exchange_order_id,
                    cancel_outcome.filled_amount,
                    cancel_outcome.source_type,
                )?,
                RequestResult::Error(error) => {
                    if error.error_type != ExchangeErrorType::ParsingError {
                        self.handle_cancel_order_failed(
                            &order.exchange_order_id,
                            error.clone(),
                            cancel_outcome.source_type,
                        )?;
                    }
                }
            };
        }

        Ok(order_cancellation_outcome)
    }

    async fn cancel_order_core(
        &self,
        // TODO Here has to be common Order (or OrderRef) cause it's more natural way:
        // When user want to cancel_order he already has that order data somewhere
        order: &OrderCancelling,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        let exchange_order_id = order.exchange_order_id.clone();
        let (tx, mut websocket_event_receiver) = oneshot::channel();

        // TODO insert is not analog of C# GetOrAd!
        // Here has to be entry().or_insert()
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
        log::info!(
            "Cancel response for {}, {:?}, {:?}",
            order.header.client_order_id,
            order.header.exchange_account_id,
            request_outcome
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
                    log::error!(
                        "raise_order_cancelled failed: unable to send thru oneshot channel: {:?}",
                        error
                    );
                }

                Ok(())
            }
            None => self.handle_cancel_order_succeeded(
                Some(&client_order_id),
                &exchange_order_id,
                filled_amount,
                source_type,
            ),
        }
    }

    pub(crate) async fn cancel_orders(
        &self,
        orders: Vec<OrderInfo>,
        cancellation_token: CancellationToken,
    ) {
        if orders.len() == 0 {
            return;
        }

        let mut futures = Vec::new();
        let mut not_found_orders = Vec::new();

        for order in orders {
            match self
                .orders
                .cache_by_exchange_id
                .get(&order.exchange_order_id)
            {
                None => not_found_orders.push(order.exchange_order_id.clone()),
                Some(order_ref) => futures.push(self.wait_cancel_order(
                    order_ref.clone(),
                    None,
                    true,
                    cancellation_token.clone(),
                )),
            }
        }

        if !not_found_orders.is_empty() {
            log::error!(
                "`cancel_orders` was received for an orders which are not in the system {}: {}",
                self.exchange_account_id,
                not_found_orders.iter().join(", "),
            );
        }

        join_all(futures).await;
    }
}
