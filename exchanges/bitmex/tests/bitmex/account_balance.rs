use crate::bitmex::bitmex_builder::BitmexBuilder;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_balance_successfully() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(true).await {
        Ok(v) => v,
        Err(_) => return,
    };

    let result = bitmex_builder
        .exchange
        .get_balance(CancellationToken::default())
        .await;

    log::info!("Balance: {result:?}");

    assert!(result.is_ok());
}
