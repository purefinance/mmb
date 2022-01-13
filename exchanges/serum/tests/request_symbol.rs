mod serum_builder;

use crate::serum_builder::SerumBuilder;
use mmb_core::exchanges::common::ExchangeAccountId;
use mmb_core::exchanges::events::AllowedEventSourceType;
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_utils::cancellation_token::CancellationToken;

#[ignore] // build_metadata works for a long time
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_symbols() {
    let exchange_account_id: ExchangeAccountId = "Serum_0".parse().expect("in test");

    let _ = SerumBuilder::try_new(
        exchange_account_id,
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
    )
    .await;
}
