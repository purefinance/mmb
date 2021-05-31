use crate::core::{
    exchanges::common::ExchangeError, orders::fill::EventSourceType,
    orders::order::ExchangeOrderId, orders::order::OrderEventType, orders::order::OrderStatus,
    orders::pool::OrderRef,
};

use super::exchange::Exchange;
use anyhow::Result;
use chrono::Utc;
use log::{error, warn};

impl Exchange {
    pub(crate) fn handle_cancel_order_failed(
        &self,
        exchange_order_id: ExchangeOrderId,
        error: ExchangeError,
        event_source_type: EventSourceType,
    ) -> Result<()> {
        if Self::should_ignore_event(
            self.features.allowed_cancel_event_source_type,
            event_source_type,
        ) {
            return Ok(());
        }

        match self.orders.cache_by_exchange_id.get(&exchange_order_id) {
            None => {
                error!("cancel_order_failed was called for an order which is not in the local order pool: {:?} on {}",
                    exchange_order_id,
                    self.exchange_account_id);

                return Ok(());
            }
            Some(order) => self.react_based_on_order_status(
                &order,
                error,
                &exchange_order_id,
                event_source_type,
            )?,
        }

        Ok(())
    }

    fn react_based_on_order_status(
        &self,
        order: &OrderRef,
        error: ExchangeError,
        exchange_order_id: &ExchangeOrderId,
        event_source_type: EventSourceType,
    ) -> Result<()> {
        match order.status() {
            OrderStatus::Canceled => {
                warn!(
                    "cancel_order_failed was called for already Canceled order: {} {:?} on {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                );

                return Ok(());
            }
            OrderStatus::Completed => {
                warn!(
                    "cancel_order_failed was called for already Completed order: {} {:?} on {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                );

                return Ok(());
            }
            _ => {
                order.fn_mut(|order| {
                    order.internal_props.last_cancellation_error = Some(error.error_type.clone());
                    order.internal_props.cancellation_event_source_type = Some(event_source_type);
                });

                self.react_based_on_error_type(
                    &order,
                    error,
                    &exchange_order_id,
                    event_source_type,
                )?;
            }
        }

        Ok(())
    }

    fn react_based_on_error_type(
        &self,
        order: &OrderRef,
        error: ExchangeError,
        exchange_order_id: &ExchangeOrderId,
        event_source_type: EventSourceType,
    ) -> Result<()> {
        match error.error_type {
            crate::core::exchanges::common::ExchangeErrorType::OrderNotFound => {
                self.handle_cancel_order_succeeded(
                    None,
                    &exchange_order_id,
                    None,
                    event_source_type,
                )?;

                return Ok(());
            }
            crate::core::exchanges::common::ExchangeErrorType::OrderCompleted => return Ok(()),
            _ => {
                if event_source_type == EventSourceType::RestFallback {
                    // TODO Some metrics
                }

                order.fn_mut(|order| order.set_status(OrderStatus::FailedToCancel, Utc::now()));
                self.add_event_on_order_change(&order, OrderEventType::CancelOrderFailed)?;

                warn!(
                    "Order cancellation failed: {} {:?} on {} with error: {:?} {:?} {}",
                    order.client_order_id(),
                    order.exchange_order_id(),
                    self.exchange_account_id,
                    error.code,
                    error.error_type,
                    error.message
                );

                // TODO DataRecorder.save()
            }
        }

        Ok(())
    }
}
