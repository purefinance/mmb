use std::{sync::Arc, time::Duration};

use mmb_domain::market::MarketAccountId;
use mmb_domain::order::snapshot::OrderSide;
use mmb_utils::{
    cancellation_token::CancellationToken,
    infrastructure::{FutureOutcome, SpawnFutureFlags},
};
use mockall_double::double;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

#[double]
use crate::balance::manager::balance_manager::BalanceManager;
#[double]
use crate::exchanges::general::engine_api::EngineApi;

use crate::infrastructure::spawn_future_timed;

pub fn close_position_if_needed(
    market_account_id: &MarketAccountId,
    balance_manager: Option<Arc<Mutex<BalanceManager>>>,
    engine_api: Arc<EngineApi>,
    cancellation_token: CancellationToken,
) -> Option<JoinHandle<FutureOutcome>> {
    match balance_manager {
        Some(balance_manager) => {
            if balance_manager
                .lock()
                .get_position(
                    market_account_id.exchange_account_id,
                    market_account_id.currency_pair,
                    OrderSide::Buy,
                )
                .is_zero()
            {
                return None;
            }
        }
        None => return None,
    }

    let action = async move {
        log::info!("Started closing active positions");
        engine_api.close_active_positions(cancellation_token).await;
        log::info!("Finished closing active positions");
        Ok(())
    };

    Some(spawn_future_timed(
        "Close active positions",
        SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
        Duration::from_secs(30),
        action,
    ))
}
