#[allow(dead_code)]
use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig};

//#[actix_web::main]
//async fn main() -> std::io::Result<()> {
//    HttpServer::new(|| App::new().service(health))
//        .bind("127.0.0.1:8080")?
//        .run()
//        .await
//}

#[actix_web::main]
async fn main() {
    let engine_config = EngineBuildConfig::standard();
    // TODO i32 is a stub, just something implementing Default
    launch_trading_engine::<i32>(&engine_config).await;
}
