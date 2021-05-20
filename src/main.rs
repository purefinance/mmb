use std::{thread, time};

use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig};

#[allow(dead_code)]
#[actix_web::main]
async fn main() {
    let engine_config = EngineBuildConfig::standard();
    launch_trading_engine::<()>(&engine_config).await;

    println!("WAITING 10 SECONDS");
    thread::sleep(time::Duration::from_secs(10));
}
