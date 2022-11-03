use mmb_utils::{cancellation_token::CancellationToken, logger::init_logger};

use super::binance_builder::BinanceBuilder;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_successfully() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(v) => v,
        Err(_) => return,
    };

    let result = binance_builder
        .exchange
        .get_balance(CancellationToken::default())
        .await;

    log::info!("Balance: {result:?}");

    assert!(result.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_successfully_futures() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0_futures().await {
        Ok(v) => v,
        Err(_) => return,
    };

    let result = binance_builder
        .exchange
        .get_balance(CancellationToken::default())
        .await;

    log::info!("Balance: {result:?}");

    assert!(result.is_ok());
}
