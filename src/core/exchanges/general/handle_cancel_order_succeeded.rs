use crate::core::{
    exchanges::common::Amount, exchanges::common::ExchangeAccountId,
    exchanges::events::AllowedEventSourceType, orders::fill::EventSourceType,
    orders::order::ClientOrderId, orders::order::ExchangeOrderId, orders::order::OrderEventType,
    orders::order::OrderStatus, orders::pool::OrderRef,
};
use anyhow::{bail, Result};
use chrono::Utc;
use log::{error, info, warn};

use super::exchange::Exchange;

impl Exchange {
    fn handle_cancel_order_succeeded(
        &self,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
        filled_amount: Option<Amount>,
        source_type: EventSourceType,
    ) -> Result<()> {
        let args_to_log = (
            self.exchange_account_id.clone(),
            exchange_order_id.clone(),
            self.features.allowed_cancel_event_source_type,
            source_type,
        );

        if Self::should_ignore_event(self.features.allowed_cancel_event_source_type, source_type) {
            info!("Ignoring fill {:?}", args_to_log);
            return Ok(());
        }

        if exchange_order_id.as_str().is_empty() {
            Self::log_cancel_handling_error_and_propagate(
                "Received HandleOrderFilled with an empty exchangeOrderId",
                &args_to_log,
            )?;
        }

        match self.orders.by_exchange_id.get(&exchange_order_id) {
            None => {
                // TODO BufferedCanceledOrderManager.add_order(exchange_order_id, self.exchange_account_id)
                // TODO All other code connected BufferedCaceledOrderManager
                Ok(())
            }
            Some(order_ref) => self.local_order_exists(
                &order_ref,
                filled_amount,
                source_type,
                client_order_id,
                exchange_order_id,
            ),
        }
    }

    fn order_already_closed(
        &self,
        status: OrderStatus,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
    ) -> bool {
        let arg_to_log = match status {
            OrderStatus::Canceled => "Canceled".to_owned(),
            OrderStatus::Completed => "Completed".to_owned(),
            _ => return false,
        };

        warn!(
            "CancelOrderSucceeded received for {} order {} {:?} {}",
            arg_to_log, client_order_id, exchange_order_id, self.exchange_account_id
        );

        true
    }

    fn local_order_exists(
        &self,
        order_ref: &OrderRef,
        filled_amount: Option<Amount>,
        source_type: EventSourceType,
        client_order_id: &ClientOrderId,
        exchange_order_id: &ExchangeOrderId,
    ) -> Result<()> {
        if self.order_already_closed(order_ref.status(), client_order_id, exchange_order_id) {
            return Ok(());
        }

        order_ref.fn_mut(|order| {
            order.internal_props.filled_amount_after_cancellation = filled_amount;
        });

        if source_type == EventSourceType::RestFallback {
            // TODO some metrics
        }

        let mut is_canceling_from_wait_cancel_order = false;
        order_ref.fn_mut(|order| {
            order.set_status(OrderStatus::Canceled, Utc::now());
            order.internal_props.cancellation_event_source_type = Some(source_type);
            is_canceling_from_wait_cancel_order =
                order.internal_props.is_canceling_from_wait_cancel_order;
        });

        // Here we cover the situation with MakerOnly orders
        // As soon as we created an order, it was automatically canceled
        // Usually we raise CancelOrderSucceeded in WaitCancelOrder after a check for fills via fallback
        // but in this particular case the cancellation is triggered by exchange itself, so WaitCancelOrder was never called
        if is_canceling_from_wait_cancel_order {
            info!("Adding CancelOrderSucceeded event from handle_cancel_order_succeeded() {} {:?} on {}",
                client_order_id,
                exchange_order_id,
                self.exchange_account_id);

            // Sometimes we start WaitCancelOrder at about the same time when as get an "order was refused/canceled" notification from an exchange (i. e. MakerOnly),
            // and we can Add CancelOrderSucceeded event here (outside WaitCancelOrder) and later from WaitCancelOrder as
            // when we check order.WasFinished in the beginning on WaitCancelOrder, the status is not set to Canceled yet
            // To avoid this situation we set CanceledNotFromWaitCancelOrder to true and then don't raise an event in WaitCancelOrder for the 2nd time
            order_ref.fn_mut(|order| {
                order.internal_props.is_canceling_from_wait_cancel_order = true;
            });

            self.add_event_on_order_change(order_ref, OrderEventType::CancelOrderSucceeded)?;
        }

        info!(
            "Order was successfully cancelled {} {:?} on {}",
            client_order_id, exchange_order_id, self.exchange_account_id
        );

        // TODO DataRecorder.save(order_ref)

        Ok(())
    }

    fn log_cancel_handling_error_and_propagate(
        template: &str,
        args_to_log: &(
            ExchangeAccountId,
            ExchangeOrderId,
            AllowedEventSourceType,
            EventSourceType,
        ),
    ) -> Result<()> {
        let error_msg = format!("{} {:?}", template, args_to_log);

        error!("{}", error_msg);
        bail!("{}", error_msg)
    }
}
