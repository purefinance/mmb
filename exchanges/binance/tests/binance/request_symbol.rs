use mmb_utils::logger::init_logger_file_named;

use crate::binance::binance_builder::BinanceBuilder;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_metadata() {
    init_logger_file_named("log.txt");

    let _ = BinanceBuilder::build_account_0().await;
}
