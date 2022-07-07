use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use dashmap::mapref::entry::Entry::{Occupied, Vacant};
use log::log;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::nothing_to_do;
use scopeguard;
use tokio::sync::broadcast;
use tokio::time::sleep;

use super::cancel::CancelOrderResult;
use crate::exchanges::{
    general::request_type::RequestType, timeouts::requests_timeout_manager::RequestGroupId,
};
use crate::{
    orders::event::OrderEventType,
    {
        exchanges::common::ExchangeError, exchanges::common::ExchangeErrorType,
        exchanges::events::AllowedEventSourceType, exchanges::general::exchange::Exchange,
        exchanges::general::exchange::RequestResult, orders::fill::EventSourceType,
        orders::order::OrderStatus, orders::pool::OrderRef,
    },
};

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
            self.create_order_created_task(order, cancellation_token.clone())
                .await?;
        }

        if order.is_finished() {
            return Ok(());
        }

        let is_canceling_from_wait_cancel_order = order.fn_mut(|order| {
            let current = order.internal_props.is_canceling_from_wait_cancel_order;
            order.internal_props.is_canceling_from_wait_cancel_order = true;
            current
        });

        if is_canceling_from_wait_cancel_order {
            log::error!(
                "Order {} {:?} is already cancelling by wait_cancel_order",
                order.client_order_id(),
                order.exchange_order_id()
            );

            return Ok(());
        }

        let order_is_finished_token = cancellation_token.create_linked_token();

        // TODO Fallback

        let mut attempt_number = 0;

        while !cancellation_token.is_cancellation_requested() {
            attempt_number += 1;

            let log_event_level = if attempt_number == 1 {
                log::Level::Info
            } else {
                log::Level::Warn
            };

            log!(
                log_event_level,
                "Cancellation iteration is {} on {} {:?} {}",
                attempt_number,
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id
            );

            self.timeout_manager
                .reserve_when_available(
                    self.exchange_account_id,
                    RequestType::CancelOrder,
                    pre_reservation_group_id,
                    order_is_finished_token.clone(),
                )?
                .await
                .into_result()?;

            let cancel_order_future = self.start_cancel_order(order, cancellation_token.clone());

            // TODO select cancel_order_task only if Exchange.AllowedCancelEventSourceType != AllowedEventSourceType.OnlyFallback

            tokio::select! {
                cancel_order_outcome = cancel_order_future, if self.features.allowed_cancel_event_source_type != AllowedEventSourceType::FallbackOnly => {
                    let cancel_order_outcome = cancel_order_outcome?;
                    self.order_cancelled(
                        order,
                        pre_reservation_group_id,
                        cancel_order_outcome,
                        cancellation_token.clone(),
                        order_is_finished_token.clone())
                        .await?;
                }
                _ = sleep(Duration::from_secs(10)) => {
                    if self.features.allowed_cancel_event_source_type != AllowedEventSourceType::All {
                        bail!("Order was expected to cancel explicitly via Rest or Web Socket but got timeout instead")
                    }

                   log::warn!("Cancel response TimedOut - re-cancelling order {} {:?} {}",
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

        let (order_cancellation_event_source_type, order_last_cancellation_error) =
            order.fn_ref(|s| {
                (
                    s.internal_props.cancellation_event_source_type,
                    s.internal_props.last_cancellation_error,
                )
            });

        log::trace!(
            "Order data in wait_cancel_order_work(): client_order_id: {}, exchange_order_id: {:?},
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

        if !order.fn_ref(|s| s.internal_props.canceled_not_from_wait_cancel_order)
            && order.status() != OrderStatus::Completed
        {
            log::info!("Adding cancel_orderSucceeded event from wait_cancel_order() for order {} {:?} on {}",
                order.client_order_id(),
                order.exchange_order_id(),
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
                    _ => {}
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
            if order.is_finished() {
                return Ok(());
            }

            order.fn_mut(|order| {
                order
                    .internal_props
                    .last_order_cancellation_status_request_time = Some(Utc::now())
            });

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
        let (order_filled_amount_after_cancellation, order_filled_amount) = order.fn_ref(|s| {
            (
                s.internal_props.filled_amount_after_cancellation,
                s.fills.filled_amount,
            )
        });

        log::info!(
            "Order with {}, {:?} order_filled_amount_after_cancellation: {:?}, order_filed_amount: {}",
            order.client_order_id(),
            order.exchange_order_id(),
            order_filled_amount_after_cancellation,
            order_filled_amount
        );

        match order_filled_amount_after_cancellation {
            Some(order_filled_amount_after_cancellation) => {
                if order_filled_amount_after_cancellation < order_filled_amount {
                    log::error!("Received order with filled amount {} less then order.filled_amount {} {} {:?} on {}",
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
