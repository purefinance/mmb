use crate::bitmex::bitmex_builder::BitmexBuilder;
use mmb_utils::logger::init_logger_file_named;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_metadata() {
    init_logger_file_named("log.txt");

    let _ = BitmexBuilder::build_account(false).await;
}
