use anyhow::Result;
use log::info;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::general::exchange::Exchange,
    orders::pool::OrderRef,
};

// TODO implement
impl Exchange {
    pub(super) async fn check_order_fills(
        &self,
        _order: &OrderRef,
        _exit_on_order_is_finished_even_if_fills_didnt_received: bool,
        _pre_reserved_group_id: Option<Uuid>,
        _cancellation_token: CancellationToken,
    ) {
    }

    pub(super) async fn create_order_finish_future(
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

        // Implement get_or_add logic
        let (tx, rx) = oneshot::channel();
        self.orders_finish_futures_by_client_order_id
            .entry(order.client_order_id())
            .or_insert(tx);

        if order.is_finished() {
            info!(
                "Exiting create_order_finish_task because order's status turned {:?} {} {:?} {}",
                order.status(),
                order.client_order_id(),
                order.exchange_order_id(),
                self.exchange_account_id
            );

            self.finish_order_future(order);

            return Ok(());
        }

        // Just wait until order cancelling future completed or operation cancelled
        tokio::select! {
            _ = rx => {}
            // TODO Evgeniy, is that compatibale according C# code?
            _ = cancellation_token.when_cancelled() => {}
        }

        Ok(())
    }

    fn finish_order_future(&self, order: &OrderRef) {
        if let Some((_, tx)) = self
            .orders_finish_futures_by_client_order_id
            .remove(&order.client_order_id())
        {
            // TODO Why do we need send order here? Mayby just () type?
            let _ = tx.send(order.clone());
        }
    }
}
