use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig};

#[allow(dead_code)]
#[actix_web::main]
async fn main() {
    let engine_config = EngineBuildConfig::standard();
    // TODO i32 is a stub, just something implementing Default
    launch_trading_engine::<i32>(&engine_config).await;
}
