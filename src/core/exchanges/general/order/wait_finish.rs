use anyhow::Result;
use log::{info, trace};
use tokio::sync::oneshot;

use crate::core::exchanges::timeouts::requests_timeout_manager::RequestGroupId;
use crate::core::{
    exchanges::general::exchange::Exchange, lifecycle::cancellation_token::CancellationToken,
    orders::pool::OrderRef,
};

// TODO implement
impl Exchange {
    pub(super) async fn check_order_fills(
        &self,
        _order: &OrderRef,
        _exit_on_order_is_finished_even_if_fills_didnt_received: bool,
        _pre_reserved_group_id: Option<RequestGroupId>,
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

        let (tx, rx) = oneshot::channel();
        self.orders_finish_events
            .entry(order.client_order_id())
            .or_insert(tx);

        if order.is_finished() {
            trace!(
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
            _ = cancellation_token.when_cancelled() => {}
        }

        Ok(())
    }

    fn finish_order_future(&self, order: &OrderRef) {
        if let Some((_, tx)) = self.orders_finish_events.remove(&order.client_order_id()) {
            let _ = tx.send(());
        }
    }
}
