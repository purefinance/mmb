use crate::Duration;
use mmb_core::lifecycle::trading_engine::EngineContext;
use mmb_domain::events::ExchangeEvent;
use mmb_utils::DateTime;
use std::sync::Arc;

pub async fn checking_orders_activity(ctx: Arc<EngineContext>) {
    let mut last_order_creation = now();

    let mut events_rx = ctx.get_events_channel();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let stop_token = ctx.lifetime_manager.stop_token();
    while !stop_token.is_cancellation_requested() {
        tokio::select! {
            event_res = events_rx.recv() => {
                match event_res {
                    Err(err) => {
                        log::error!("Error occurred: {err:?}");
                        ctx.lifetime_manager.spawn_graceful_shutdown("Error in start_liquidity_order_book_saving");
                    }
                    Ok(ExchangeEvent::OrderEvent(_)) => {
                        last_order_creation = now();
                    }
                    Ok(_) => check_timeout(last_order_creation, &ctx),
                }
            }
            _ = interval.tick() => check_timeout(last_order_creation, &ctx),
        }
    }
}

fn check_timeout(last_order_creation: DateTime, ctx: &EngineContext) {
    let max_timeout_without_order_creation = Duration::minutes(5);

    if now() - last_order_creation > max_timeout_without_order_creation {
        log::error!(
            "There is no orders activity during {} min",
            max_timeout_without_order_creation.num_minutes(),
        );

        ctx.lifetime_manager
            .spawn_graceful_shutdown("There is no orders activity too long");
    }
}

fn now() -> DateTime {
    chrono::Utc::now()
}
