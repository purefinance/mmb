use std::time::Duration;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType, exchanges::events::AllowedEventSourceType,
    exchanges::general::exchange::Exchange, exchanges::general::exchange::RequestResult,
    orders::fill::EventSourceType, orders::order::ExchangeOrderId, orders::order::OrderEventType,
    orders::order::OrderStatus, orders::pool::OrderRef,
};
use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use log::{error, info, warn};
use scopeguard;
use tokio::sync::oneshot;
use tokio::time::sleep;
use uuid::Uuid;

impl Exchange {
    pub async fn wait_cancel_order(
        &self,
        order: OrderRef,
        pre_reservation_group_id: Option<Uuid>,
        check_order_fills: bool,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        info!(
            "Executing wait_cancel_order() with order: {} {:?} {}",
            order.client_order_id(),
            order.exchange_order_id(),
            self.exchange_account_id,
        );

        // Implement get_or_add logic
        match self
            .wait_cancel_order_by_client_order_id
            .entry(order.client_order_id())
        {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                let rx = entry.get_mut();
                // Just wait until order cancelling future completed or operation cancelled
                tokio::select! {
                    _ = rx => {}
                    _ = cancellation_token.when_cancelled() => {}
                }
            }
            dashmap::mapref::entry::Entry::Vacant(vacant_entry) => {
                // Be sure value will be removed anyway
                let _guard = scopeguard::guard((), |_| {
                    let _ = self
                        .wait_cancel_order_by_client_order_id
                        .remove(&order.client_order_id());
                });
                let (tx, rx) = oneshot::channel();

                let outcome = self
                    .wait_cancel_order_work(
                        &order,
                        pre_reservation_group_id,
                        check_order_fills,
                        cancellation_token,
                    )
                    .await;

                let _ = tx.send(outcome);
                let _ = *vacant_entry.insert(rx);
            }
        }

        Ok(())
    }

    async fn wait_cancel_order_work(
        &self,
        order: &OrderRef,
        pre_reservation_group_id: Option<Uuid>,
        check_order_fills: bool,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if order.status() == OrderStatus::Creating {
            self.create_order_created_task(order, cancellation_token.clone())
                .await?;
        }

        if order.is_finished() {
            return Ok(());
        }

        if order.is_canceling_from_wait_cancel_order() {
            error!(
                "Order {} {:?} is already cancelling by waitt_cancel_order",
                order.client_order_id(),
                order.exchange_order_id()
            );

            return Ok(());
        }

        order.fn_mut(|order| order.internal_props.is_canceling_from_wait_cancel_order = true);

        let order_is_finished_token = cancellation_token.create_linked_token();

        // TODO Fallback

        let mut attempts_number = 0;

        while !cancellation_token.is_cancellation_requested() {
            attempts_number += 1;

            let log_event_level = if attempts_number == 1 {
                log::Level::Info
            } else {
                log::Level::Warn
            };

            log::log!(
                log_event_level,
                "Cancellation iteration is {} on {} {:?} {}",
                attempts_number,
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id
            );

            // TODO timeout_manager.reserver_when_available()

            let order_to_cancel = order
                .to_order_cancelling()
                .ok_or(anyhow!("Order has no exchange order id"))?;

            let cancel_order_task = self.cancel_order(&order_to_cancel, cancellation_token.clone());

            // TODO select cance_order_task only if Exchange.AllowedCancelEventSourceType != AllowedEventSourceType.OnlyFallback

            // FIXME
            let cancel_delay = Duration::from_secs(10);
            let timeout_future = sleep(cancel_delay);
            tokio::select! {
                cancel_order_outcome = cancel_order_task => {
                    info!("Cancel order future finished first on order {}, {:?} {}",
                        order.client_order_id(),
                        order.exchange_order_id(),
                        self.exchange_account_id);

                    if let  Some(cancel_order_outcome) = cancel_order_outcome {
                        if let RequestResult::Error(error) = cancel_order_outcome.outcome {
                            match error.error_type {
                                ExchangeErrorType::ParsingError => {
                                    self.check_order_cancellation_status(order, Some(error), pre_reservation_group_id, cancellation_token.clone()).await?;
                                }
                                ExchangeErrorType::PendingError => {
                                    sleep(error.pending_time).await;
                                }
                                ExchangeErrorType::OrderCompleted => {
                                    // Happens when an order is completed while we are waiting for cancellation
                                    // For exchanges with order_was_completed_error_for_cancellation feature is ignore
                                    // cancellatio error (otherwise we have a chance of skipping a fill) and without
                                    // order_finish_task we would exit wait_cancel_order() only via fallback which is slow
                                    self.create_order_finish_future(order, order_is_finished_token.clone()).await?;
                                }
                                _ => {}
                            }
                        }

                    }
                }
                _ = timeout_future => {
                    if self.features.allowed_cancel_event_source_type != AllowedEventSourceType::All {
                        bail!("Order was expected to cancel explicity via Rest or Web Socket but got timeout instead")
                    }

                    warn!("Cancel response TimedOut - re-cancelling order {} {:?} {}",
                        order.client_order_id(),
                        order.exchange_order_id(),
                        self.exchange_account_id);
                }
                // TODO select Fallback future
            };

            if order.is_finished() {
                order_is_finished_token.cancel();
                break;
            }
        }

        let order_has_missed_fills = self.has_missed_fill(order);

        let order_cancellation_event_source_type =
            order.internal_props().cancellation_event_source_type;
        let order_last_cancellation_error = order.internal_props().last_cancellation_error;

        info!(
            "client_order_id: {}, exchange_order_id: {:?},
            checked_order_fills: {}, order_has_missed_fills: {:?},
            order_cancellation_event_source_type: {:?}, last_cancellation_error: {:?},
            order_status: {:?}",
            order.client_order_id(),
            order.exchange_order_id(),
            check_order_fills,
            order_has_missed_fills,
            order_cancellation_event_source_type,
            order_last_cancellation_error,
            order.status()
        );

        if check_order_fills
            || order_has_missed_fills
            // If cancellation notification received via fallback, there is a chance web socket is not functioning and fill notification was missed
            || order_cancellation_event_source_type == Some(EventSourceType::RestFallback)
            || (order_cancellation_event_source_type == Some(EventSourceType::WebSocket)
            || order_cancellation_event_source_type == Some(EventSourceType::Rest)
            && (order_last_cancellation_error == Some(ExchangeErrorType::OrderNotFound)
            // If cancellation received not from a fallback but order not found / compltytd bit !order.is_completed, there is a chance fill notification was missed
            || order_last_cancellation_error == Some(ExchangeErrorType::OrderCompleted)))
            && order.status() != OrderStatus::Completed
        {
            self.check_order_fills(
                order,
                false,
                pre_reservation_group_id,
                cancellation_token.clone(),
            )
            .await;
        }

        if order.internal_props().canceled_not_from_wait_cancel_order
            && order.status() != OrderStatus::Completed
        {
            info!("Adding cancel_orderSucceeded event from wait_cancel_order() fro order {} {:?} on {}",
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id);

            self.add_event_on_order_change(order, OrderEventType::CancelOrderSucceeded)?;
        }

        Ok(())
    }

    // FIXME implement
    async fn create_order_created_task(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if order.status() != OrderStatus::Creating {
            info!("Instantly exiting create_order_created_task brcause order's statis is {:?} {} {:?} on {}",
                order.status(),
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id);

            return Ok(());
        }

        cancellation_token.error_if_cancellation_requested()?;

        // FIXME something with TaskCompletionSource

        if order.status() != OrderStatus::Creating {
            info!("Exiting create_order_created_task because order's status turned {:?} while tcs were creating {} {:?} on {}",
                order.status(),
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id);

            self.create_order_task(order);
            return Ok(());
        }

        // FIXME let currentTcs = new TaskCompletionSource...

        Ok(())
    }

    // FIXME implement
    // FIXME and move to other file
    // FIXME rename all task to future
    fn create_order_task(&self, _order: &OrderRef) {
        // FIXME implement
        //if self.order_created_task.try_remove(order.client_order_id())

        // FIXME HealthCheckStorage.mark_event()
    }

    // FIXME implement
    async fn check_order_cancellation_status(
        &self,
        order: &OrderRef,
        exchange_error: Option<ExchangeError>,
        pre_reserved_group_id: Option<Uuid>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        while !cancellation_token.is_cancellation_requested() {
            if order.is_finished() {
                return Ok(());
            }

            // FIXME Does the DateTimeService needed?
            order.fn_mut(|order| {
                order
                    .internal_props
                    .last_order_cancellation_status_request_time = Some(Utc::now())
            });

            // FIXME Add TimeoutManager
            // let reverve_result = TimeoutManger...

            if order.is_finished() {
                return Ok(());
            }

            info!(
                "Checking order status in check_order_cancellation_status with order {} {:?} {}",
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id
            );

            let cloned_order = order.deep_clone();
            let order_info = self.get_order_info(&cloned_order).await;

            if order.is_finished() {
                return Ok(());
            }

            match order_info {
                Err(error) => {
                    if error.error_type == ExchangeErrorType::OrderNotFound {
                        let new_error = match exchange_error {
                            Some(gotten_error) => gotten_error,
                            None => ExchangeError::new(
                                ExchangeErrorType::Unknown,
                                "There are no any response from an exchange, so probably this order was not canceling".to_owned(),
                                None)
                        };

                        self.handle_cancel_order_failed(
                            order.exchange_order_id(),
                            new_error,
                            EventSourceType::RestFallback,
                        );

                        break;
                    }

                    warn!(
                        "Error for order_info was received {} {:?} {} {:?} {:?}",
                        order.client_order_id(),
                        order.exchange_order_id(),
                        self.exchange_account_id,
                        order.currency_pair(),
                        error
                    );

                    continue;
                }
                Ok(order_info) => {
                    match order_info.order_status {
                        OrderStatus::Canceled => {
                            if let Some(exchange_order_id) = order.exchange_order_id() {
                                self.handle_cancel_order_succeeded(
                                    &order.client_order_id(),
                                    &exchange_order_id,
                                    Some(order_info.filled_amount),
                                    EventSourceType::RestFallback,
                                )?;
                            }
                        }
                        OrderStatus::Completed => {
                            // Looks like we've missed a fill while we were cancelling, it can happen in two scenarios:
                            // 1. Test ShouldCheckFillsForCompletedOrders. There we clear a completed order to be able to
                            // test a case of cancelling a completed order which involves calling CheckOrderFills in case of OrderCompleted
                            // 2. We've received OrderCompleted during cancelling but a fill message was lost
                            self.check_order_fills(
                                order,
                                false,
                                pre_reserved_group_id,
                                cancellation_token,
                            )
                            .await;
                        }
                        _ => {}
                    }

                    break;
                }
            }
        }

        Ok(())
    }

    // FIXME implement
    // FIXME move to other file
    fn handle_cancel_order_failed(
        &self,
        _exchange_order_id: Option<ExchangeOrderId>,
        _error: ExchangeError,
        _event_source_type: EventSourceType,
    ) {
    }

    // FIXME implement
    async fn create_order_finish_future(
        &self,
        order: &OrderRef,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if order.is_finished() {
            info!(
                "Instantly exiting create_order_finish_future() because status is {:?} {} {:?} {}",
                order.status(),
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id
            );

            return Ok(());
        }

        cancellation_token.error_if_cancellation_requested()?;

        // FIXME continue here with TaskCompletionSource
        Ok(())
    }

    fn has_missed_fill(&self, order: &OrderRef) -> bool {
        let order_filled_amount_after_cancellation =
            order.internal_props().filled_amount_after_cancellation;
        let (_, order_filled_amount) = order.get_fills();

        info!(
            "Order with {}, {:?} order_filled_amount_after_cancellatio: {:?}, order_filed_amount: {}",
            order.client_order_id(),
            order.exchange_order_id(),
            order_filled_amount_after_cancellation,
            order_filled_amount
        );

        match order_filled_amount_after_cancellation {
            Some(order_filled_amount_after_cancellation) => {
                if order_filled_amount_after_cancellation < order_filled_amount {
                    error!("Received order with filled amount {} less then order.filled_amount {} {} {:?} on {}",
                        order_filled_amount_after_cancellation,
                        order_filled_amount,
                        order.client_order_id(),
                        order.exchange_order_id(),
                        self.exchange_account_id);

                    return false;
                }

                order_filled_amount_after_cancellation > order_filled_amount
            }
            None => false,
        }
    }
}
