use std::time::Duration;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::general::exchange::Exchange,
    exchanges::general::exchange::RequestResult, orders::order::OrderStatus,
    orders::pool::OrderRef,
};
use anyhow::{anyhow, Result};
use log::{error, trace};
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
        trace!(
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
                log::Level::Trace
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

            let cancel_delay = Duration::from_secs(10);
            let timeout_future = sleep(cancel_delay);
            tokio::select! {
                cancel_order_outcome = cancel_order_task => {
                    trace!("Cancel order future finished first on order {}, {:?} {}",
                        order.client_order_id(),
                        order.exchange_order_id(),
                        self.exchange_account_id);

                    if let  Some(cancel_order_outcome) = cancel_order_outcome {
                        if let RequestResult::Error(error) = cancel_order_outcome.outcome {

                            // FIXME continue here
                        }

                    }
                }
                // TODO select Fallback future
            };
        }

        Ok(())
    }

    async fn create_order_created_task(
        &self,
        _order: &OrderRef,
        _cancellation_token: CancellationToken,
    ) {
    }
}
