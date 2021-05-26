use uuid::Uuid;

use crate::core::{
    exchanges::cancellation_token::CancellationToken, exchanges::general::exchange::Exchange,
    orders::pool::OrderRef,
};

// FIXME implement
impl Exchange {
    pub(super) async fn check_order_fills(
        &self,
        _order: &OrderRef,
        _exit_on_order_is_finished_even_if_fills_didnt_received: bool,
        _pre_reserved_group_id: Option<Uuid>,
        _cancellation_token: CancellationToken,
    ) {
    }
}
