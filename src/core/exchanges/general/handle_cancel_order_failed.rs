use crate::core::{
    exchanges::common::ExchangeError, orders::fill::EventSourceType, orders::order::ExchangeOrderId,
};

use super::exchange::Exchange;
use anyhow::Result;
use log::{error, info, warn};

impl Exchange {
    // TODO implement
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
            Some(order) => match order.status() {
                crate::core::orders::order::OrderStatus::Canceled => {
                    warn!(
                        "cancel_order_failed was called for already Canceled order: {} {:?} on {}",
                        order.client_order_id(),
                        order.exchange_order_id(),
                        self.exchange_account_id,
                    );

                    return Ok(());
                }
                crate::core::orders::order::OrderStatus::Completed => {
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
                        order.internal_props.last_cancellation_error =
                            Some(error.error_type.clone());
                        order.internal_props.cancellation_event_source_type =
                            Some(event_source_type);
                    });

                    match error.error_type {
                        crate::core::exchanges::common::ExchangeErrorType::OrderNotFound => {
                            self.handle_cancel_order_succeeded(
                                None,
                                &exchange_order_id,
                                None,
                                event_source_type,
                            )?;
                        }
                        crate::core::exchanges::common::ExchangeErrorType::OrderCompleted => {}
                        _ => {}
                    }
                }
            },
        }

        // FIXME Delete
        Ok(())
    }
}
