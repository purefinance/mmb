use mmb::exchanges::{events::AllowedEventSourceType, general::commission::Commission};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;

use crate::binance::binance_builder::BinanceBuilder;

#[actix_rt::test]
async fn request_metadata() {
    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    // build_metadata is called in try_new, so if it's doesn't panicked metadata fetched successfully
    let _ = BinanceBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
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
