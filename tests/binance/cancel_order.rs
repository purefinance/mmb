use std::collections::HashMap;

use chrono::Utc;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, general::commission::Commission};
use mmb_lib::core::exchanges::{common::*, timeouts::timeout_manager::TimeoutManager};
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use tokio::sync::broadcast;

use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;

use rust_decimal_macros::dec;
#[actix_rt::test]
async fn cancelled_successfully() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::default());
    let (tx, _rx) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];
    let binance = Box::new(Binance::new(
        exchange_account_id.clone(),
        settings.clone(),
        tx.clone(),
        application_manager.clone(),
    ));

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        binance,
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

    let order = crate::core::order::Order::new(
        None,
        exchange_account_id.clone(),
        Some("FromCancelOrderTest".to_string()),
        CancellationToken::default(),
    );

    // Should be called before any other api calls!
    exchange.build_metadata().await;

    match order.create(exchange.clone()).await {
        Ok(order_ref) => {
            order.cancel(&order_ref, exchange).await;
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn nothing_to_cancel() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::default());
    let (tx, _) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];

    let binance = Binance::new(
        exchange_account_id.clone(),
        settings,
        tx.clone(),
        application_manager.clone(),
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

    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let generated_client_order_id = ClientOrderId::unique_id();
    let order_header = OrderHeader::new(
        generated_client_order_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(10000),
        OrderExecutionType::None,
        None,
        None,
        "FromCancelOrderTest".to_owned(),
    );

    let order_to_cancel = OrderCancelling {
        header: order_header,
        exchange_order_id: "1234567890".into(),
    };

    // Should be called before any other api calls!
    exchange.build_metadata().await;
    // Cancel last order
    let cancel_outcome = exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test")
        .expect("in test");

    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
