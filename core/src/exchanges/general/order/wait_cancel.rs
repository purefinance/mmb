use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use dashmap::mapref::entry::Entry::{Occupied, Vacant};
use futures::pin_mut;
use log::log;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::nothing_to_do;
use scopeguard;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

use super::cancel::CancelOrderResult;
use crate::exchanges::common::ToStdExpected;
use crate::exchanges::{
    general::request_type::RequestType, timeouts::requests_timeout_manager::RequestGroupId,
};
use crate::misc::time::time_manager;
use crate::{
    orders::event::OrderEventType,
    {
        exchanges::common::ExchangeError, exchanges::common::ExchangeErrorType,
        exchanges::events::AllowedEventSourceType, exchanges::general::exchange::Exchange,
        exchanges::general::exchange::RequestResult, orders::fill::EventSourceType,
        orders::order::OrderStatus, orders::pool::OrderRef,
    },
};

const CANCEL_DELAY: Duration = Duration::from_secs(10);

impl Exchange {
    pub async fn wait_cancel_order(
        &self,
        order: OrderRef,
        pre_reservation_group_id: Option<RequestGroupId>,
        check_order_fills: bool,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        log::info!(
            "Executing wait_cancel_order() with order: {} {:?} {}",
            order.client_order_id(),
            order.exchange_order_id(),
            self.exchange_account_id,
        );

        // we move rx out of the closure to unlock Dashmap while waiting
        let rx = match self.wait_cancel_order.entry(order.client_order_id()) {
            Occupied(entry) => Some(entry.get().subscribe()),
            Vacant(vacant_entry) => {
                // Be sure value will be removed anyway
                let _guard = scopeguard::guard((), |_| {
                    let _ = self.wait_cancel_order.remove(&order.client_order_id());
                });
                let (tx, _) = broadcast::channel(1);
                let _ = *vacant_entry.insert(tx.clone());

                self.wait_cancel_order_work(
                    &order,
                    pre_reservation_group_id,
                    check_order_fills,
                    cancellation_token.clone(),
                )
                .await?;

                let _ = tx.send(());
                None
            }
        };

        if let Some(mut rx) = rx {
            tokio::select! {
                _ = rx.recv() => nothing_to_do(),
                _ = cancellation_token.when_cancelled() => nothing_to_do()
            }
        }
        Ok(())
    }

    async fn wait_cancel_order_work(
        &self,
        order: &OrderRef,
        pre_reservation_group_id: Option<RequestGroupId>,
        check_order_fills: bool,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if order.status() == OrderStatus::Creating {
            self.create_order_created_fut(order, cancellation_token.clone())
                .await?;
        }

        let (is_canceling_from_wait_cancel_order, is_finished, client_order_id, exchange_order_id) =
            order.fn_mut(|order| {
                let current = order.internal_props.is_canceling_from_wait_cancel_order;
                order.internal_props.is_canceling_from_wait_cancel_order = true;

                (
                    current,
                    order.is_finished(),
                    order.client_order_id(),
                    order.exchange_order_id(),
                )
            });

        if is_finished {
            return Ok(());
        }

        if is_canceling_from_wait_cancel_order {
            log::error!("Order {client_order_id} {exchange_order_id:?} is already cancelling by wait_cancel_order");

            return Ok(());
        }

        let order_is_finished_token = cancellation_token.create_linked_token();

        let poll_cancellation_fut = {
            //In background we poll for fills every x seconds for those rare cases when we missed a WebSocket fill
            let poll_fut = self.poll_order_cancellation_status(
                order.clone(),
                pre_reservation_group_id,
                order_is_finished_token.clone(),
            );

            let duration = Duration::from_secs(3 * 60 * 60);
            async move {
                timeout(duration, poll_fut).await.unwrap_or_else(|_| bail!("Time in form of {duration:?} is over, but future `poll wait cancel order` is not completed yet"))
            }
        };

        let is_poll_enabled = self.features.websocket_options.cancellation_notification
            && self.features.allowed_cancel_event_source_type
                != AllowedEventSourceType::NonFallback;

        pin_mut!(poll_cancellation_fut);

        let mut attempt_number = 0;
        while !cancellation_token.is_cancellation_requested() {
            attempt_number += 1;

            let log_event_level = match attempt_number == 1 {
                true => log::Level::Trace,
                false => log::Level::Warn,
            };

            log!(log_event_level, "Cancellation iteration is {attempt_number} on {client_order_id} {exchange_order_id:?} {}", self.exchange_account_id);

            self.timeout_manager
                .reserve_when_available(
                    self.exchange_account_id,
                    RequestType::CancelOrder,
                    pre_reservation_group_id,
                    order_is_finished_token.clone(),
                )?
                .await
                .into_result()?;

            let cancel_order_fut = self.start_cancel_order(order, cancellation_token.clone());
            pin_mut!(cancel_order_fut);

            let mut cancel_order_fut_enabled = true;
            loop {
                tokio::select! {
                    cancel_order_outcome = &mut cancel_order_fut, if cancel_order_fut_enabled => {
                        // FallbackOnly only for testing fallback work. In this case we need start cancellation, but skipping handling cancel_order_fut result
                        if self.features.allowed_cancel_event_source_type != AllowedEventSourceType::FallbackOnly {
                            self.order_cancelled(
                                order,
                                pre_reservation_group_id,
                                cancel_order_outcome?,
                                cancellation_token.clone(),
                                order_is_finished_token.clone())
                                .await?;
                        } else {
                            cancel_order_fut_enabled = false;

                            // continue polling fallback without polling cancel_order_fut
                            continue;
                        }
                    }
                    _ = sleep(CANCEL_DELAY) => {
                        if self.features.allowed_cancel_event_source_type != AllowedEventSourceType::All {
                            bail!("Order was expected to cancel explicitly via Rest or Web Socket but got timeout instead")
                        }

                       log::warn!("Cancel response TimedOut - re-cancelling order {client_order_id} {exchange_order_id:?} {}", self.exchange_account_id);
                    }
                    poll_result = &mut poll_cancellation_fut, if is_poll_enabled => {
                        let level = match poll_result {
                            Ok(()) => log::Level::Trace,
                            Err(_) => log::Level::Error,
                        };

                        let error_part = match poll_result {
                            Ok(()) => String::new(),
                            Err(err) => format!("with result: {err:?}"),
                        };

                        log!(level, "'poll_order_cancellation_status_fut' finished first {client_order_id} {exchange_order_id:?} {} {error_part}", self.exchange_account_id);
                    }
                };

                break;
            }

            if order.is_finished() {
                order_is_finished_token.cancel();
                break;
            }
        }

        let order_has_missed_fills = self.has_missed_fill(order);

        let (
            exchange_order_id,
            order_cancellation_event_source_type,
            order_last_cancellation_error,
            status,
        ) = order.fn_ref(|s| {
            (
                s.exchange_order_id(),
                s.internal_props.cancellation_event_source_type,
                s.internal_props.last_cancellation_error,
                s.props.status,
            )
        });

        log::trace!(
            "Order data in wait_cancel_order_work(): client_order_id: {client_order_id}, exchange_order_id: {exchange_order_id:?},
            checked_order_fills: {check_order_fills}, order_has_missed_fills: {order_has_missed_fills:?},
            order_cancellation_event_source_type: {order_cancellation_event_source_type:?}, last_cancellation_error: {order_last_cancellation_error:?},
            order_status: {status:?}");

        if check_order_fills
            || order_has_missed_fills
            // If cancellation notification received via fallback, there is a chance web socket is not functioning and fill notification was missed
            || order_cancellation_event_source_type == Some(EventSourceType::RestFallback)
            || (order_cancellation_event_source_type == Some(EventSourceType::WebSocket)
            || order_cancellation_event_source_type == Some(EventSourceType::Rest)
            && (order_last_cancellation_error == Some(ExchangeErrorType::OrderNotFound)
            // If cancellation received not from a fallback but order not found / completed bit !order.is_completed, there is a chance fill notification was missed
            || order_last_cancellation_error == Some(ExchangeErrorType::OrderCompleted)))
            && order.status() != OrderStatus::Completed
        {
            self.check_order_fills(
                order,
                false,
                pre_reservation_group_id,
                cancellation_token.clone(),
            )
            .await?;
        }

        let cancelled_order = order.fn_ref(|s| {
            (!s.internal_props.canceled_not_from_wait_cancel_order  //i. e. an order was refused by an exchange
            && s.props.status != OrderStatus::Completed)
                .then(|| (s.client_order_id(), s.exchange_order_id()))
        });

        if let Some((client_order_id, exchange_order_id)) = cancelled_order {
            log::trace!("Adding CancelOrderSucceeded event from wait_cancel_order() for order {} {:?} on {}",
                client_order_id,
                exchange_order_id,
                self.exchange_account_id);

            self.add_event_on_order_change(order, OrderEventType::CancelOrderSucceeded)?;
        }

        Ok(())
    }

    async fn order_cancelled(
        &self,
        order: &OrderRef,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancel_order_outcome: Option<CancelOrderResult>,
        cancellation_token: CancellationToken,
        order_is_finished_token: CancellationToken,
    ) -> Result<()> {
        log::info!(
            "Cancel order future finished first on order {}, {:?} {}",
            order.client_order_id(),
            order.exchange_order_id(),
            self.exchange_account_id
        );

        if let Some(cancel_order_outcome) = cancel_order_outcome {
            if let RequestResult::Error(error) = cancel_order_outcome.outcome {
                match error.error_type {
                    ExchangeErrorType::ParsingError => {
                        self.check_order_cancellation_status(
                            order,
                            Some(error),
                            pre_reservation_group_id,
                            cancellation_token.clone(),
                        )
                        .await?;
                    }
                    ExchangeErrorType::PendingError(pending_time) => {
                        sleep(pending_time).await;
                    }
                    ExchangeErrorType::OrderCompleted => {
                        // Happens when an order is completed while we are waiting for cancellation
                        // For exchanges with order_was_completed_error_for_cancellation feature is ignore
                        // cancellation error (otherwise we have a chance of skipping a fill) and without
                        // order_finish_task we would exit wait_cancel_order() only via fallback which is slow
                        self.create_order_finish_future(order, order_is_finished_token.clone())
                            .await?;
                    }
                    _ => nothing_to_do(),
                }
            }
        }

        Ok(())
    }

    async fn check_order_cancellation_status(
        &self,
        order: &OrderRef,
        exchange_error: Option<ExchangeError>,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        while !cancellation_token.is_cancellation_requested() {
            let is_finished = order.fn_mut(|order| {
                if order.is_finished() {
                    true
                } else {
                    order
                        .internal_props
                        .last_order_cancellation_status_request_time = Some(Utc::now());

                    false
                }
            });

            if is_finished {
                return Ok(());
            }

            self.timeout_manager
                .reserve_when_available(
                    self.exchange_account_id,
                    RequestType::CancelOrder,
                    pre_reservation_group_id,
                    cancellation_token.clone(),
                )?
                .await
                .into_result()?;

            if order.is_finished() {
                return Ok(());
            }

            log::trace!(
                "Checking order status in check_order_cancellation_status with order {} {:?} {}",
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id
            );

            let order_info = self.get_order_info(order).await;

            if order.is_finished() {
                return Ok(());
            }

            match order_info {
                Err(error) => {
                    if error.error_type == ExchangeErrorType::OrderNotFound {
                        let new_error = exchange_error.unwrap_or_else(|| ExchangeError::new(
                            ExchangeErrorType::Unknown,
                            "There are no any response from an exchange, so probably this order was not canceling".to_owned(),
                            None)
                        );

                        let exchange_order_id = order.exchange_order_id().with_context(|| {
                            format!(
                                "There are no exchange_order_id in order {} {:?} on {}",
                                order.client_order_id(),
                                order.exchange_order_id(),
                                self.exchange_account_id,
                            )
                        })?;

                        self.handle_cancel_order_failed(
                            &exchange_order_id,
                            new_error,
                            EventSourceType::RestFallback,
                        );

                        break;
                    }

                    log::warn!(
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
                                    Some(&order.client_order_id()),
                                    &exchange_order_id,
                                    Some(order_info.filled_amount),
                                    EventSourceType::RestFallback,
                                );
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
                                pre_reservation_group_id,
                                cancellation_token,
                            )
                            .await?;
                        }
                        _ => nothing_to_do(),
                    }

                    break;
                }
            }
        }

        Ok(())
    }

    fn has_missed_fill(&self, order: &OrderRef) -> bool {
        let (
            client_order_id,
            exchange_order_id,
            order_filled_amount_after_cancellation,
            order_filled_amount,
        ) = order.fn_ref(|s| {
            (
                s.client_order_id(),
                s.exchange_order_id(),
                s.internal_props.filled_amount_after_cancellation,
                s.fills.filled_amount,
            )
        });

        log::trace!("Order with {client_order_id}, {exchange_order_id:?} order_filled_amount_after_cancellation: {order_filled_amount_after_cancellation:?}, order_filed_amount: {order_filled_amount}");

        match order_filled_amount_after_cancellation {
            Some(order_filled_amount_after_cancellation) => {
                if order_filled_amount_after_cancellation < order_filled_amount {
                    log::error!("Received order with filled amount {order_filled_amount_after_cancellation} less then order.filled_amount {order_filled_amount} {client_order_id} {exchange_order_id:?} on {}", self.exchange_account_id);
                    return false;
                }

                order_filled_amount_after_cancellation > order_filled_amount
            }
            None => false,
        }
    }

    async fn poll_order_cancellation_status(
        &self,
        order: OrderRef,
        pre_reservation_group_id: Option<RequestGroupId>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        while !cancellation_token.is_cancellation_requested() {
            let (is_finished, last_order_creation_status_request_time) = order.fn_ref(|o| {
                (
                    o.is_finished(),
                    o.internal_props.last_order_creation_status_request_time,
                )
            });

            if is_finished {
                return Ok(());
            }

            let now = time_manager::now();

            let order_cancellation_status_request_period = chrono::Duration::seconds(5);
            let delay_till_fallback_request = match last_order_creation_status_request_time {
                None => Some(order_cancellation_status_request_period.to_std_expected()),
                Some(last_time) => (order_cancellation_status_request_period - (now - last_time))
                    .to_std()
                    .ok(),
            };

            if let Some(delay_till_fallback_request) = delay_till_fallback_request {
                tokio::select! {
                    _ = sleep(delay_till_fallback_request) => nothing_to_do(),
                    _ = cancellation_token.when_cancelled() => return Ok(()),
                }
            }

            //If an order was canceled while we were waiting for the timeout, we don't need to request fills for it
            if order.fn_ref(|o| o.is_finished()) {
                return Ok(());
            }

            self.check_order_cancellation_status(
                &order,
                None,
                pre_reservation_group_id,
                cancellation_token.clone(),
            )
            .await?;
        }

        Ok(())
    }
}
