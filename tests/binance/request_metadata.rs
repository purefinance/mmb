use std::collections::HashMap;

use mmb::{
    exchanges::{
        events::AllowedEventSourceType, general::commission::Commission,
        timeouts::timeout_manager::TimeoutManager,
    },
    statistic_service::StatisticService,
};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::binance::binance::*;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::settings;

use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use tokio::sync::broadcast;

#[actix_rt::test]
async fn request_metadata() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, mut _rx) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);

    settings.websocket_channels = vec!["depth".into(), "trade".into()];
    let binance = Binance::new(
        exchange_account_id.clone(),
        settings,
        tx.clone(),
        application_manager.clone(),
        StatisticService::new(),
    );

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        Box::new(binance),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        application_manager,
        TimeoutManager::new(HashMap::new()),
        Commission::default(),
    );

    exchange.clone().connect().await;

    // If it's not panicked metadata fetched successfully
    exchange.build_metadata().await;
}
