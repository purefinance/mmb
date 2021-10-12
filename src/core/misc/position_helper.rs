use std::{sync::Arc, time::Duration};

use futures::FutureExt;
use parking_lot::Mutex;

use crate::core::{
    balance_manager::balance_manager::BalanceManager, exchanges::common::TradePlaceAccount,
    infrastructure::spawn_future_timed, orders::order::OrderSide,
};

pub async fn close_position_if_needed(
    trade_place: &TradePlaceAccount,
    balance_manager: Arc<Mutex<BalanceManager>>,
    // TODO: fix when close_position will implemented
    // IBotApi _botApi;
) {
    if balance_manager
        .lock()
        .get_position(
            &trade_place.exchange_account_id,
            &trade_place.currency_pair,
            OrderSide::Buy,
        )
        .is_zero()
    {
        return;
    }
    let action = async {
        log::info!("Started closing active positions");
        // await botApi.CloseActivePositions();
        log::info!("Finished closing active positions");
        Ok(())
    };

    let action_name = "Close active positions";
    spawn_future_timed(action_name, true, Duration::from_secs(30), action.boxed())
        .await
        .expect(format!("Failed to run '{}'", action_name).as_str()); // TODO: fix me
}
