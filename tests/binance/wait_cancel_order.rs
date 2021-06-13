use crate::get_binance_credentials_or_exit;
use chrono::Utc;
use mmb_lib::core::exchanges::application_manager::ApplicationManager;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, general::commission::Commission};
use mmb_lib::core::exchanges::{
    cancellation_token::CancellationToken, events::AllowedEventSourceType,
};
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use tokio::sync::broadcast;
use tokio::time::Duration;

use super::common::get_timeout_manager;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;

#[actix_rt::test]
async fn cancellation_waited_successfully() {
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
    let (tx, _) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];

    let binance = Box::new(Binance::new(
        exchange_account_id.clone(),
        settings,
        tx.clone(),
        application_manager.clone(),
    )) as BoxExchangeClient;

    let timeout_manager = get_timeout_manager(&exchange_account_id);

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
        timeout_manager,
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
            // If here are no error - order was cancelled successfully
            exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await
                .expect("in test");
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn cancellation_waited_failed_fallback() {
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
    let (tx, _) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];

    let binance = Box::new(Binance::new(
        exchange_account_id.clone(),
        settings,
        tx.clone(),
        application_manager.clone(),
    )) as BoxExchangeClient;

    let timeout_manager = get_timeout_manager(&exchange_account_id);

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        binance,
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::FallbackOnly,
        ),
        tx,
        application_manager,
        timeout_manager,
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
            let must_be_error = exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await;
            match must_be_error {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Order was expected to cancel explicity via Rest or Web Socket but got timeout instead",
                        &error.to_string()[..85]
                    );
                }
            }
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}
