use crate::serum::serum_builder::SerumBuilder;

#[ignore] // build_metadata works for a long time
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_symbols() {
    let serum_builder = SerumBuilder::build_account_0().await;
    let exchange = serum_builder.exchange;

    assert!(!exchange.symbols.is_empty());
    assert!(exchange.currencies.lock().len() > 0);
}
