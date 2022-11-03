#![cfg(test)]
use crate::get_binance_credentials_or_exit;
use binance::binance::BinanceBuilder;
use mmb_core::config::parse_settings;
use mmb_core::infrastructure::spawn_future_ok;
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_utils::infrastructure::SpawnFutureFlags;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn launch_engine() {
    let config = EngineBuildConfig::new(vec![Box::new(BinanceBuilder)]);

    let (api_key, secret_key) = get_binance_credentials_or_exit!();
    let credentials =
        format!("[Binance_0]\napi_key = \"{api_key}\"\nsecret_key = \"{secret_key}\"");

    let settings = match parse_settings::<TestStrategySettings>(
        include_str!("lifecycle.toml"),
        &credentials,
    ) {
        Ok(settings) => settings,
        Err(_) => return, // For CI, while we cant setup keys on github
    };

    let init_settings = InitSettings::Directly(settings);
    let engine = launch_trading_engine(&config, init_settings)
        .await
        .expect("in tests");

    let context = engine.context();

    let action = async move {
        sleep(Duration::from_millis(200)).await;
        context.lifetime_manager.run_graceful_shutdown("test").await;
    };
    spawn_future_ok(
        "run graceful_shutdown in launch_engine test",
        SpawnFutureFlags::DENY_CANCELLATION,
        action,
    );

    engine.run().await;
}
