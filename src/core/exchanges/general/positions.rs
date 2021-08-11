use std::sync::Arc;

use crate::core::exchanges::general::exchange::Exchange;
use crate::core::exchanges::general::request_type::RequestType;
use crate::core::exchanges::get_active_position::ActivePosition;
use crate::core::lifecycle::cancellation_token::CancellationToken;
async fn close_position() {}

async fn get_active_positions(exchange: Arc<Exchange>) -> Vec<ActivePosition> {
    loop {
        exchange.clone().timeout_manager.reserve_when_available(
            &exchange.exchange_account_id,
            RequestType::GetActivePositions,
            None,
            CancellationToken::default(), // TODO: probably need to fix
        );
        if let Ok(positions) = exchange.get_active_positions().await {
            return positions;
        }
    }
}
