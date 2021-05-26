use std::time::Duration;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::common::ExchangeError,
    exchanges::common::ExchangeErrorType, exchanges::events::AllowedEventSourceType,
    exchanges::general::exchange::Exchange, exchanges::general::exchange::RequestResult,
    orders::fill::EventSourceType, orders::order::OrderEventType, orders::order::OrderStatus,
    orders::pool::OrderRef,
};
use anyhow::{anyhow, bail, Result};
use log::{error, info, warn};
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

        // FIXME is that really analog of C# GetOrAdd? Or AddOrUpdate?
        //self.futures_to_wait_cancel_order_by_client_order_id.insert(order.client_order_id(), )

        let _result = self
            .wait_cancel_order_work(
                &order,
                pre_reservation_group_id,
                check_order_fills,
                cancellation_token,
            )
            .await;

        // FIXME try-catch-finally

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
            // FIXME todo
            self.create_order_created_task(order, cancellation_token.clone())
                .await;
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
                                    self.check_order_cancellation_status(order, &error, pre_reservation_group_id, cancellation_token.clone()).await;
                                }
                                ExchangeErrorType::PendingError => {
                                    sleep(error.pending_time).await;
                                }
                                ExchangeErrorType::OrderCompleted => {
                                    // Happens when an order is completed while we are waiting for cancellation
                                    // For exchanges with order_was_completed_error_for_cancellation feature is ignore
                                    // cancellatio error (otherwise we have a chance of skipping a fill) and without
                                    // order_finish_task we would exit wait_cancel_order() only via fallback which is slow
                                    self.create_order_finish_future(order, order_is_finished_token.clone()).await;
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

        // FIXME Maybe _sync_object
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
        _order: &OrderRef,
        _cancellation_token: CancellationToken,
    ) {
    }

    // FIXME implement
    async fn check_order_cancellation_status(
        &self,
        _order: &OrderRef,
        _error: &ExchangeError,
        _pre_reserved_group_id: Option<Uuid>,
        _cancellation_token: CancellationToken,
    ) {
    }

    // FIXME implement
    async fn create_order_finish_future(
        &self,
        _order: &OrderRef,
        _cancellation_token: CancellationToken,
    ) {
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
