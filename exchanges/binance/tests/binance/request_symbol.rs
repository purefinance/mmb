use mmb_core::exchanges::common::*;
use mmb_core::exchanges::general::features::*;
use mmb_core::exchanges::{events::AllowedEventSourceType, general::commission::Commission};
use mmb_utils::cancellation_token::CancellationToken;

use crate::binance::binance_builder::BinanceBuilder;

#[actix_rt::test]
async fn request_metadata() {
    let exchange_account_id: ExchangeAccountId = "Binance_0".parse().expect("in test");
    // build_symbol is called in try_new, so if it's doesn't panicked symbol fetched successfully
    let _ = BinanceBuilder::try_new(
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
        true,
    )
    .await;
}
