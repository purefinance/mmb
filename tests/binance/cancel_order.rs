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
use rust_decimal_macros::*;
use tokio::sync::broadcast;
use tokio::time::Duration;

use super::common::get_timeout_manager;
use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;

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

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
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

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: dec!(0.0000001),
    };

    // Should be called before any other api calls!
    exchange.build_metadata().await;
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    const TIMEOUT: Duration = Duration::from_secs(5);
    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    match created_order {
        Ok(order_ref) => {
            let exchange_order_id = order_ref.exchange_order_id().expect("in test");
            let order_to_cancel = OrderCancelling {
                header: order_header,
                exchange_order_id,
            };
            // Cancel last order
            let cancel_outcome = exchange
                .cancel_order(&order_to_cancel, CancellationToken::default())
                .await
                .expect("in test")
                .expect("in test");

            if let RequestResult::Success(gotten_client_order_id) = cancel_outcome.outcome {
                assert_eq!(gotten_client_order_id, test_order_client_id);
            }
        }
        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}
#[actix_rt::test]
async fn cancel_opened_orders_successfully() {
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

    let timeout_manager = get_timeout_manager(&exchange_account_id);
    let exchange = Exchange::new(
        exchange_account_id.clone(),
        binance,
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        application_manager,
        timeout_manager,
        Commission::default(),
    );

    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let test_price = dec!(0.0000001);
    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(2000),
        OrderExecutionType::None,
        None,
        None,
        "FromCancelOrderTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: test_price,
    };

    // Should be called before any other api calls!
    exchange.build_metadata().await;
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());
    const TIMEOUT: Duration = Duration::from_secs(5);
    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    match created_order {
        Ok(_order_ref) => {
            let second_test_order_client_id = ClientOrderId::unique_id();
            let second_order_header = OrderHeader::new(
                second_test_order_client_id.clone(),
                Utc::now(),
                exchange_account_id.clone(),
                test_currency_pair.clone(),
                OrderType::Limit,
                OrderSide::Buy,
                dec!(2000),
                OrderExecutionType::None,
                None,
                None,
                "FromCancelOrderTest".to_owned(),
            );

            let second_order_to_create = OrderCreating {
                header: second_order_header,
                price: test_price,
            };

            let created_order_fut =
                exchange.create_order(&second_order_to_create, CancellationToken::default());

            let _ = tokio::select! {
                created_order = created_order_fut => created_order,
                _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
            }.expect("in test");
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }

    match &exchange.get_open_orders().await {
        Err(error) => {
            log::log!(
                log::Level::Info,
                "Opened orders not found for exchange account id: {}",
                error,
            );
            assert!(false);
        }
        Ok(orders) => {
            assert_ne!(orders.len(), 0);
            &exchange
                .clone()
                .cancel_opened_orders(CancellationToken::default())
                .await;
        }
    }

    match &exchange.get_open_orders().await {
        Err(error) => {
            log::log!(
                log::Level::Info,
                "Opened orders not found for exchange account id: {}",
                error,
            );
            assert!(false);
        }
        Ok(orders) => {
            assert_eq!(orders.len(), 0);
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
    let cancel_outcome = exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test")
        .expect("in test");
    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
