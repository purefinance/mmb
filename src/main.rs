use std::time::Duration;

use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig};

#[allow(dead_code)]
#[actix_web::main]
async fn main() {
    let engine_config = EngineBuildConfig::standard();
    let _engine_context = launch_trading_engine::<()>(&engine_config).await;

    // TODO repace it with event loop in the future
    //engine_context
    //    .application_manager
    //    .clone()
    //    .spawn_graceful_shutdown("test".to_owned());

    // TODO repace it with event loop in the future
    tokio::time::sleep(Duration::from_secs(10)).await;
    dbg!(&"AFTER SLEEP");
}
