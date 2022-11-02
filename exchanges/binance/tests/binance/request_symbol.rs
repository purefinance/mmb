use mmb_utils::logger::init_logger;

use crate::binance::binance_builder::BinanceBuilder;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_metadata() {
    init_logger();

    let _ = BinanceBuilder::build_account_0().await;
}
