use std::time::Duration;

use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig};

#[allow(dead_code)]
#[actix_web::main]
async fn main() {
    let engine_config = EngineBuildConfig::standard();
    launch_trading_engine::<()>(&engine_config).await;

    tokio::time::sleep(Duration::from_secs(20)).await;
    dbg!(&"AFTER SLEEP");
}
