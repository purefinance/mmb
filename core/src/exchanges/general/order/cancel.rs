use anyhow::{anyhow, Result};
use chrono::Utc;
use futures::future::join_all;
use itertools::Itertools;
use mmb_utils::cancellation_token::CancellationToken;
use tokio::sync::oneshot;

use crate::{
    exchanges::common::Amount,
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
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
                let order_cancellation_outcome =
                    self.cancel_order(order_to_cancel, cancellation_token).await;

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
        order: OrderCancelling,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        let exchange_order_id = order.exchange_order_id.clone();
        let order_cancellation_outcome = self.cancel_order_core(order, cancellation_token).await;

        // Option is returning when cancel_order_core is stopped by CancellationToken
        // So approptiate Handler was already called in a fallback
        if let Some(ref cancel_outcome) = order_cancellation_outcome {
            match &cancel_outcome.outcome {
                RequestResult::Success(client_order_id) => self.handle_cancel_order_succeeded(
                    Some(client_order_id),
                    &exchange_order_id,
                    cancel_outcome.filled_amount,
                    cancel_outcome.source_type,
                ),
                RequestResult::Error(error) => {
                    if error.error_type != ExchangeErrorType::ParsingError {
                        self.handle_cancel_order_failed(
                            &exchange_order_id,
                            error.clone(),
                            cancel_outcome.source_type,
                        );
                    }
                }
            };
        }

        order_cancellation_outcome
    }

    async fn cancel_order_core(
        &self,
        // TODO Here has to be common Order (or OrderRef) cause it's more natural way:
        // When user want to cancel_order he already has that order data somewhere
        order: OrderCancelling,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        let exchange_order_id = order.exchange_order_id.clone();
        let (tx, mut websocket_event_receiver) = oneshot::channel();

        // TODO insert is not analog of C# GetOrAd!
        // Here has to be entry().or_insert()
        self.order_cancellation_events
            .insert(exchange_order_id.clone(), (tx, None));

        let cancel_order_future = self.exchange_client.cancel_order(order);

        tokio::select! {
            cancel_order_result = cancel_order_future => {
                match cancel_order_result.outcome {
                    RequestResult::Error(_) => {
                        // TODO if ExchangeFeatures.Order.CreationResponseFromRestOnlyForError
                        Some(cancel_order_result)
                    }
                    RequestResult::Success(_) => {
                        tokio::select! {
                            websocket_outcome = &mut websocket_event_receiver => {
                                websocket_outcome.ok()
                            }
                            _ = cancellation_token.when_cancelled() => {
                                None
                            }
                        }
                    }
                }
            }
            _ = cancellation_token.when_cancelled() => {
                None
            }
            websocket_outcome = &mut websocket_event_receiver => {
                websocket_outcome.ok()
            }
        }
    }

    pub(crate) fn raise_order_cancelled(
        &self,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        source_type: EventSourceType,
    ) {
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
        if orders.is_empty() {
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
