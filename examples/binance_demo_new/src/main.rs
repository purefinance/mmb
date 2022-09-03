#![deny(
    non_ascii_idents,
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_must_use,
    clippy::unwrap_used
)]

mod orders_activity;

use anyhow::Result;
use binance::binance::BinanceBuilder;
use chrono::Duration;
use mmb_core::config::{CONFIG_PATH, CREDENTIALS_PATH};
use mmb_core::infrastructure::{spawn_future, spawn_future_ok};
use mmb_core::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::settings::BaseStrategySettings;
use mmb_utils::infrastructure::SpawnFutureFlags;
use strategies::example_strategy::{ExampleStrategy, ExampleStrategySettings};
use vis_robot_integration::start_visualization_data_saving;

const STRATEGY_NAME: &str = "binance_demo";

#[tokio::main]
async fn main() -> Result<()> {
    let engine_config = EngineBuildConfig::new(vec![Box::new(BinanceBuilder)]);

    let init_settings = InitSettings::<ExampleStrategySettings>::Load {
        config_path: CONFIG_PATH.to_owned(),
        credentials_path: CREDENTIALS_PATH.to_owned(),
    };
    loop {
        let engine =
            launch_trading_engine(&engine_config, init_settings.clone(), |settings, ctx| {
                spawn_future(
                    "Save visualization data",
                    SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                    start_visualization_data_saving(ctx.clone(), STRATEGY_NAME),
                );

                spawn_future_ok(
                    "Checking orders activity",
                    SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                    orders_activity::checking_orders_activity(ctx.clone()),
                );

                Box::new(ExampleStrategy::new(
                    settings.strategy.exchange_account_id(),
                    settings.strategy.currency_pair(),
                    settings.strategy.spread,
                    settings.strategy.max_amount,
                    ctx,
                ))
            })
            .await?;

        match engine.run().await {
            ActionAfterGracefulShutdown::Nothing => break,
            ActionAfterGracefulShutdown::Restart => continue,
        }
    }
    Ok(())
}
