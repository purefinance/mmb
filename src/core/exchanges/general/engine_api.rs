use std::sync::Arc;

use futures::future::join_all;
use itertools::Itertools;
#[cfg(test)]
use mockall::automock;
#[cfg(test)]
use parking_lot::{Mutex, MutexGuard};

use crate::core::{
    exchanges::common::ClosedPosition, lifecycle::cancellation_token::CancellationToken,
};

use super::exchange::Exchange;

pub struct EngineApi {
    exchange: Arc<Exchange>,
}

#[cfg_attr(test, automock)]
impl EngineApi {
    pub async fn close_active_positions(
        &self,
        cancellation_token: CancellationToken,
    ) -> Vec<ClosedPosition> {
        log::info!(
            "Closing active position for exchange {}",
            self.exchange.exchange_account_id
        );

        let active_positions = self
            .exchange
            .get_active_positions(cancellation_token.clone())
            .await;

        let get_closed_positions_futures = active_positions
            .iter()
            .filter_map(|active_position| {
                if active_position.derivative.position.is_zero() {
                    return None;
                }
                Some(self.exchange.close_position_loop(
                    active_position,
                    None,
                    cancellation_token.clone(),
                ))
            })
            .collect_vec();

        let closed_positions = join_all(get_closed_positions_futures).await;

        log::info!(
            "Closed active position for exchange {}",
            self.exchange.exchange_account_id
        );

        closed_positions
    }
}

crate::impl_mock_initializer!(MockEngineApi, ENGINE_API_MOCK_LOCKER);
