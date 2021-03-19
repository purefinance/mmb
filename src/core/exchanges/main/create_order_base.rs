use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use futures::pin_mut;
use log::{error, info, warn};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::core::{
    exchanges::cancellation_token::CancellationToken,
    exchanges::common::ExchangeAccountId,
    exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType,
    orders::order::ClientOrderId,
    orders::order::ExchangeOrderId,
    orders::order::OrderEventType,
    orders::order::OrderSnapshot,
    orders::order::OrderStatus,
    orders::order::OrderType,
    orders::pool::OrderRef,
    orders::{fill::EventSourceType, order::OrderCreating},
};

use super::{create_order::CreateOrderResult, exchange::Exchange, exchange::RequestResult};
use crate::core::exchanges::main::exchange::RequestResult::{Error, Success};

impl Exchange {
    pub async fn create_order(
        &self,
        order: &OrderSnapshot,
        cancellation_token: CancellationToken,
        // FIXME Evgeniy, look at this signature. Maybe it should be OrderRef?
    ) -> Result<OrderRef> {
        info!("Submitting order {:?}", order);
        self.orders
            .add_snapshot_initial(Arc::new(RwLock::new(order.clone())));

        let _linked_cancellation_token =
            CancellationToken::create_linked_token(&cancellation_token);

        let create_order_future = self.create_order_base(order, cancellation_token);

        pin_mut!(create_order_future);
        // TODO if AllowedCreateEventSourceType != AllowedEventSourceType.OnlyFallback
        // TODO self.poll_order_create(order, pre_reservation_group_id, _linked_cancellation_token)

        tokio::select! {
            created_order_outcome = create_order_future => {
                match created_order_outcome {
                    Ok(created_order_result) => {
                        self.match_created_order_outcome(&created_order_result.outcome, order)
                    }
                    Err(_exchange_error) => {
                        // FIXME How to handle it?????
                        bail!("Some exchange error")
                    }
                }
            }
        }
    }

    fn match_created_order_outcome(
        &self,
        outcome: &RequestResult<ExchangeOrderId>,
        order: &OrderSnapshot,
    ) -> Result<OrderRef> {
        match outcome {
            Success(exchange_order_id) => {
                let result_order = &*self
                    .orders
                    .orders_by_exchange_id
                    .get(&exchange_order_id).ok_or_else(|| anyhow!("Impossible situation: order was created, but missing in local orders pool"))?;

                // TODO create_order_cancellation_token_source.cancel();

                // TODO check_order_fills(order...)

                if order.props.status == OrderStatus::Creating {
                    error!(
                        "OrderStatus of order {} is Creating at the end of create order procedure",
                        order.header.client_order_id
                    );
                }

                // TODO DataRecorder.Save(order); Do we really need it here?
                // Cause it's already performed in handle_create_order_succeeded

                info!(
                    "Order was submitted {} {:?} {:?} on {}",
                    order.header.client_order_id,
                    order.props.exchange_order_id,
                    order.header.reservation_id,
                    order.header.exchange_account_id
                );

                return Ok(result_order.clone());
            }
            Error(exchange_error) => {
                if exchange_error.error_type == ExchangeErrorType::ParsingError {
                    // TODO Error handling should be placed in self.check_order_creation().await
                }
                dbg!(&exchange_error);
                bail!("Delete it in the future")
            }
        }
    }

    async fn create_order_base(
        &self,
        order: &OrderSnapshot,
        cancellation_token: CancellationToken,
    ) -> Result<CreateOrderResult> {
        let order_to_create = OrderCreating {
            header: (*order.header).clone(),
            price: order.props.price(),
        };
        let create_order_result = self
            .create_order_core(&order_to_create, cancellation_token)
            .await;

        if let Some(created_order) = create_order_result {
            match &created_order.outcome {
                Success(exchange_order_id) => {
                    self.handle_create_order_succeeded(
                        &self.exchange_account_id,
                        &order.header.client_order_id,
                        &exchange_order_id,
                        &created_order.source_type,
                    )?;
                }
                Error(exchange_error) => {
                    if exchange_error.error_type != ExchangeErrorType::ParsingError {
                        self.handle_create_order_failed(
                            &self.exchange_account_id,
                            &order.header.client_order_id,
                            &exchange_error,
                            &created_order.source_type,
                        )?
                    }
                }
            }

            return Ok(created_order);
        }

        // FIXME Hm, in practice order already can be created on exchange
        bail!("Order wasn't created, task was stopped earlier")
    }

    // FIXME should be part of BotBase?
    fn handle_create_order_failed(
        &self,
        exchange_account_id: &ExchangeAccountId,
        client_order_id: &ClientOrderId,
        exchange_error: &ExchangeError,
        source_type: &EventSourceType,
    ) -> Result<()> {
        // TODO some lock? Why should we?
        // TODO implement should_ignore_event() in the future cause there are some fallbacks handling

        let args_to_log = (exchange_account_id, client_order_id);

        if client_order_id.as_str().is_empty() {
            let error_msg = format!(
                "Order was created but client_order_id is empty. Order: {:?}",
                args_to_log
            );

            error!("{}", error_msg);
            bail!("{}", error_msg);
        }

        match self.orders.orders_by_client_id.get(client_order_id) {
            None => {
                let error_msg = format!(
                "CreateOrderSucceeded was received for an order which is not in the local orders pool {:?}",
                args_to_log
            );
                error!("{}", error_msg);

                bail!("{}", error_msg);
            }
            Some(order_ref) => order_ref.fn_mut(|order| {
                let args_to_log = (
                    exchange_account_id,
                    client_order_id,
                    &order.props.exchange_order_id.clone(),
                );
                self.react_on_status_when_failed(order, args_to_log, source_type, exchange_error)
            }),
        }
    }

    fn react_on_status_when_failed(
        &self,
        order: &mut OrderSnapshot,
        args_to_log: (&ExchangeAccountId, &ClientOrderId, &Option<ExchangeOrderId>),
        _source_type: &EventSourceType,
        exchange_error: &ExchangeError,
    ) -> Result<()> {
        let status = order.props.status;
        match status {
            OrderStatus::Created => Self::log_error_and_propagate("Created", args_to_log),
            OrderStatus::FailedToCreate => {
                warn!(
                    "CreateOrderSucceeded was received for a FaildeToCreate order {:?}",
                    args_to_log
                );
                Ok(())
            }
            OrderStatus::Canceling => Self::log_error_and_propagate("Canceling", args_to_log),
            OrderStatus::Canceled => Self::log_error_and_propagate("Canceled", args_to_log),
            OrderStatus::Completed => Self::log_error_and_propagate("Completed", args_to_log),
            OrderStatus::FailedToCancel => {
                // FIXME what about this option? Why it did not handled in C#?
                Self::log_error_and_propagate("FailedToCancel", args_to_log)
            }
            OrderStatus::Creating => {
                // TODO RestFallback and some metrics

                order.set_status(OrderStatus::FailedToCreate, Utc::now());
                order.internal_props.last_creation_error_type =
                    Some(exchange_error.error_type.clone());
                order.internal_props.last_creation_error_message = exchange_error.message.clone();

                self.add_event_on_order_change(order, OrderEventType::CreateOrderFailed);

                // TODO DataRecorder.Save(order)

                warn!(
                    "Order creation failed {:?}, with error: {:?}",
                    args_to_log, exchange_error
                );

                Ok(())
            }
        }
    }

    fn log_error_and_propagate(
        template: &str,
        args_to_log: (&ExchangeAccountId, &ClientOrderId, &Option<ExchangeOrderId>),
    ) -> Result<()> {
        let error_msg = format!(
            "CreateOrderFailed was received for a {} order {:?}",
            template, args_to_log
        );

        error!("{}", error_msg);
        bail!("{}", error_msg)
    }

    fn handle_create_order_succeeded(
        &self,
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

            error!("{}", error_msg);
            bail!("{}", error_msg);
        }

        if exchange_order_id.as_str().is_empty() {
            let error_msg = format!(
                "Order was created but exchange_order_id is empty. Order: {:?}",
                args_to_log
            );

            error!("{}", error_msg);
            bail!("{}", error_msg);
        }

        match self.orders.orders_by_client_id.get(client_order_id) {
            None => {
                warn!("CreateOrderSucceeded was received for an order which is not in the local orders pool {:?}", args_to_log);

                return Ok(());
            }
            Some(order_ref) => order_ref.fn_mut(|order| {
                order.props.exchange_order_id = Some(exchange_order_id.clone());

                self.react_on_status_when_succeed(order, args_to_log, &order_ref, source_type)
            }),
        }
    }

    fn log_warn(
        template: &str,
        args_to_log: (&ExchangeAccountId, &ClientOrderId, &ExchangeOrderId),
    ) -> Result<()> {
        warn!(
            "CreateOrderSucceeded was received for a {} order {:?}",
            template, args_to_log
        );
        Ok(())
    }

    fn react_on_status_when_succeed(
        &self,
        order: &mut OrderSnapshot,
        args_to_log: (&ExchangeAccountId, &ClientOrderId, &ExchangeOrderId),
        // Use it only for inserting in orders_pool
        order_ref: &OrderRef,
        source_type: &EventSourceType,
    ) -> Result<()> {
        let status = order.props.status;
        let exchange_order_id = args_to_log.2;
        match status {
            OrderStatus::FailedToCreate => {
                let error_msg = format!(
                                "CreateOrderSucceeded was received for a FailedToCreate order.
                                Probably FaildeToCreate fallbach was received before Creation Rresponse {:?}",
                                args_to_log
                            );

                error!("{}", error_msg);
                bail!("{}", error_msg)
            }
            OrderStatus::Created => Self::log_warn("Created", args_to_log),
            OrderStatus::Canceling => Self::log_warn("Canceling", args_to_log),
            OrderStatus::Canceled => Self::log_warn("Canceled", args_to_log),
            OrderStatus::Completed => Self::log_warn("Completed", args_to_log),
            OrderStatus::FailedToCancel => Self::log_warn("FailedToCancel", args_to_log),
            OrderStatus::Creating => {
                if self
                    .orders
                    .orders_by_exchange_id
                    .contains_key(exchange_order_id)
                {
                    info!(
                        "Order has already been added to the local orders pool {:?}",
                        args_to_log
                    );

                    return Ok(());
                }

                // TODO RestFallback and some metrics

                order.set_status(OrderStatus::Created, Utc::now());
                order.internal_props.creation_event_source_type = Some(source_type.clone());
                self.orders
                    .orders_by_exchange_id
                    .insert(exchange_order_id.clone(), order_ref.clone());

                if order.header.order_type != OrderType::Liquidation {
                    // TODO BalanceManager
                }

                self.add_event_on_order_change(order, OrderEventType::CreateOrderSucceeded);

                // TODO if BufferedFillsManager.TryGetFills(...)
                // TODO if BufferedCanceledORdersManager.TrygetOrder(...)

                // TODO DataRecorder.Save(order); Do we really need it here?
                // Cause it's already performed in handle_create_order_succeeded

                info!("Order was created: {:?}", args_to_log);

                Ok(())
            }
        }
    }

    // TODO Should be in botBase?
    fn add_event_on_order_change(&self, order: &mut OrderSnapshot, event_type: OrderEventType) {
        if event_type == OrderEventType::CancelOrderSucceeded {
            order.internal_props.cancellation_event_was_raised = true;
        }

        if order.props.is_finished() {
            let _ = self
                .orders
                .not_finished_orders
                .remove(&order.header.client_order_id);
        }

        // TODO events_channel.add_event(new ORderEvent);
    }
}
