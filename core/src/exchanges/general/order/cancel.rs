use anyhow::Result;
use futures::future::join_all;
use itertools::Itertools;
use mmb_domain::events::EventSourceType;
use mmb_domain::market::ExchangeErrorType;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::Amount;
use mmb_domain::order::snapshot::{ClientOrderId, ExchangeOrderId, OrderInfo, OrderStatus};
use mmb_utils::cancellation_token::CancellationToken;
use tokio::sync::oneshot;

use crate::exchanges::traits::ExchangeError;
use crate::misc::time::time_manager;
use crate::{exchanges::general::exchange::Exchange, exchanges::general::exchange::RequestResult};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CancelOrderResult {
    pub outcome: RequestResult<ClientOrderId>,
    pub source_type: EventSourceType,
    // TODO Use it in the future
    pub filled_amount: Option<Amount>,
}

impl CancelOrderResult {
    pub fn succeed(
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
        let client_order_id = order.client_order_id();
        let (status, exchange_order_id) = order.fn_ref(|x| (x.status(), x.exchange_order_id()));
        match status {
            OrderStatus::Canceled => {
                log::info!(
                    "Order {client_order_id} {exchange_order_id:?} are already canceled on {}",
                    self.exchange_account_id
                );
                Ok(None)
            }
            OrderStatus::Completed => {
                log::info!(
                    "Order {client_order_id} {exchange_order_id:?} are already completed on {}",
                    self.exchange_account_id
                );
                Ok(None)
            }
            _ => {
                order.fn_mut(|order| order.set_status(OrderStatus::Canceling, time_manager::now()));

                log::info!(
                    "Submitting order cancellation {client_order_id} {exchange_order_id:?} on {}",
                    self.exchange_account_id
                );

                let order_cancellation_outcome = self.cancel_order(order, cancellation_token).await;

                log::info!(
                    "Submitted order cancellation {client_order_id} {exchange_order_id:?} on {}: {order_cancellation_outcome:?}", self.exchange_account_id);

                Ok(order_cancellation_outcome)
            }
        }
    }

    pub async fn cancel_order(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        match order.exchange_order_id() {
            Some(exchange_order_id) => {
                let order_cancellation_outcome = self
                    .cancel_order_core(order, &exchange_order_id, cancellation_token)
                    .await;

                // Option is returning when cancel_order_core is stopped by CancellationToken
                // So appropriate Handler was already called in a fallback
                if let Some(ref cancel_outcome) = order_cancellation_outcome {
                    match &cancel_outcome.outcome {
                        RequestResult::Success(client_order_id) => self
                            .handle_cancel_order_succeeded(
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
            None => {
                log::warn!("Missing exchange_order_id in cancelling order");
                None
            }
        }
    }

    async fn cancel_order_core(
        &self,
        order: &OrderRef,
        exchange_order_id: &ExchangeOrderId,
        cancellation_token: CancellationToken,
    ) -> Option<CancelOrderResult> {
        let (tx, mut websocket_event_receiver) = oneshot::channel();

        // TODO insert is not analog of C# GetOrAd!
        // Here has to be entry().or_insert()
        self.order_cancellation_events
            .insert(exchange_order_id.clone(), (tx, None));

        let cancel_order_future = self.exchange_client.cancel_order(order, exchange_order_id);

        tokio::select! {
            cancel_order_result = cancel_order_future => {
                match cancel_order_result.outcome {
                    RequestResult::Error(_) => {
                        // TODO if ExchangeFeatures.Order.CreationResponseFromRestOnlyForError
                        Some(cancel_order_result)
                    }
                    RequestResult::Success(_) => {
                        tokio::select! {
                            websocket_outcome = &mut websocket_event_receiver => websocket_outcome.ok(),
                            _ = cancellation_token.when_cancelled() => None,
                        }
                    }
                }
            }
            _ = cancellation_token.when_cancelled() => None,
            websocket_outcome = &mut websocket_event_receiver => websocket_outcome.ok(),
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
                let send_res = tx.send(CancelOrderResult::succeed(
                    client_order_id,
                    source_type,
                    filled_amount,
                ));
                if let Err(err) = send_res {
                    log::error!("raise_order_cancelled failed: unable to send thru oneshot channel: {err:?}");
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
