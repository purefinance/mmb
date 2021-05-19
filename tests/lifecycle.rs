use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig};
use std::time::Duration;
use tokio::time::sleep;

#[actix_rt::test]
async fn launch_engine() {
    let config = EngineBuildConfig::standard();
    let engine_context = launch_trading_engine::<()>(&config).await;

    sleep(Duration::from_millis(200)).await;

    engine_context
        .application_manager
        .run_graceful_shutdown("test")
        .await;
}
