use mmb_utils::{cancellation_token::CancellationToken, logger::init_logger_file_named};

use super::binance_builder::BinanceBuilder;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_successfully() {
    init_logger_file_named("log.txt");

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(v) => v,
        Err(_) => return,
    };

    let result = binance_builder
        .exchange
        .get_balance(CancellationToken::default())
        .await;

    log::info!("Balance: {:?}", result);

    assert!(result.is_some());
}
