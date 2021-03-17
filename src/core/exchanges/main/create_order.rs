use anyhow::{bail, Result};
use futures::pin_mut;
use log::{error, info};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::core::{
    exchanges::cancellation_token::CancellationToken,
    exchanges::common::ExchangeAccountId,
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
    exchanges::common::RestRequestOutcome,
    orders::order::ClientOrderId,
    orders::order::ExchangeOrderId,
    orders::order::OrderSnapshot,
    orders::order::OrderStatus,
    orders::{fill::EventSourceType, order::OrderCreating},
};

use super::{exchange::Exchange, exchange::RequestResult};
use crate::core::exchanges::main::exchange::RequestResult::{Error, Success};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CreateOrderResult {
    pub outcome: RequestResult<ExchangeOrderId>,
    pub source_type: EventSourceType,
}

impl CreateOrderResult {
    pub fn successed(order_id: ExchangeOrderId, source_type: EventSourceType) -> Self {
        CreateOrderResult {
            outcome: RequestResult::Success(order_id),
            source_type,
        }
    }

    pub fn failed(error: ExchangeError, source_type: EventSourceType) -> Self {
        CreateOrderResult {
            outcome: RequestResult::Error(error),
            source_type,
        }
    }
}

impl Exchange {
    // FIXME think about better name
    pub async fn create_order_base(
        &self,
        order: &OrderSnapshot,
        cancellation_token: CancellationToken,
    ) {
        info!("Submitting order {:?}", order);
        //let order_ref: OrderRef = OrderRef(Arc::new(RwLock::new(order)));
        self.orders
            .add_snapshot_initial(Arc::new(RwLock::new(order.clone())));

        // FIXME handle cancellation_token

        let create_order_future = self.create_order(order, cancellation_token);

        pin_mut!(create_order_future);
        // TODO if AllowedCreateEventSourceType != AllowedEventSourceType.OnlyFallback

        tokio::select! {
            created_order_outcome = create_order_future => {
                if let Some(created_order_result) = created_order_outcome{
                    if let Error(exchange_error) = created_order_result.outcome {
                        if exchange_error.error_type == ExchangeErrorType::ParsingError {
                            // FIXME self.check_order_creation()
                        }
                    }
                }
            }
        }

        // TODO check_order_fills(order...)

        if order.props.status == OrderStatus::Creating {
            error!(
                "OrderStatus of order {} is Creating at the end of create order procedure",
                order.header.client_order_id
            );
        }

        // TODO DataRecorder.Save(order);

        info!(
            "Order was submitted {} {:?} {:?} on {}",
            order.header.client_order_id,
            order.props.exchange_order_id,
            order.header.reservation_id,
            order.header.exchange_account_id
        );
    }

    pub async fn create_order(
        &self,
        order: &OrderSnapshot,
        cancellation_token: CancellationToken,
    ) -> Option<CreateOrderResult> {
        let order_to_create = OrderCreating {
            header: (*order.header).clone(),
            price: order.props.price(),
        };
        let create_order_result = self
            .create_order_core(&order_to_create, cancellation_token)
            .await;

        if let Some(created_order) = create_order_result {
            match created_order.outcome {
                Success(exchange_order_id) => {
                    Self::handle_create_order_succeeded(
                        &self.exchange_account_id,
                        &order.header.client_order_id,
                        &exchange_order_id,
                        &created_order.source_type,
                    );
                }
                Error(exchange_error) => {
                    if exchange_error.error_type != ExchangeErrorType::ParsingError {
                        // FIXME handle_create_order_failed()
                    }
                }
            }
        }

        // FIXME return value
        None
    }

    // FIXME should be part of BotBase?
    fn handle_create_order_succeeded(
        exchange_account_id: &ExchangeAccountId,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
        source_type: &EventSourceType,
    ) -> Result<()> {
        // TODO some lock? Why should we?
        // TODO implement should_ignore_event() in the future cause there are some fallbacks handling

        let args_to_log = (exchange_account_id, client_order_id, exchange_order_id);

        if client_order_id.as_str().is_empty() {
            let error_msg = format!(
                "Order was created but client_order_id is empty. Order: {:?}",
                args_to_log
            );
            // FIXME do we really new log here, or it just wil be performed caller side?
            error!("{}", error_msg);

            bail!("{}", error_msg);
        }

        bail!("delete this bail")
    }

    pub async fn create_order_core(
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

    pub(super) fn raise_order_created(
        &self,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        source_type: EventSourceType,
    ) {
        if let Some((_, (tx, _))) = self.order_creation_events.remove(&client_order_id) {
            if let Err(error) =
                tx.send(CreateOrderResult::successed(exchange_order_id, source_type))
            {
                error!("Unable to send thru oneshot channel: {:?}", error);
            }
        }
    }
}
