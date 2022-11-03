use crate::bitmex::bitmex_builder::BitmexBuilder;
use mmb_utils::logger::init_logger;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_metadata() {
    init_logger();

    let _ = BitmexBuilder::build_account(false).await;
}
