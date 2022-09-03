use crate::serum::serum_builder::SerumBuilder;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_successfully() {
    init_logger_file_named("log.txt");

    let serum_builder = SerumBuilder::build_account_0().await;

    let result = serum_builder
        .exchange
        .get_balance(CancellationToken::default())
        .await;

    log::info!("Balance: {result:?}");

    assert!(result.is_ok());
}
