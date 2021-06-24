use std::collections::HashMap;

use chrono::Utc;
use mmb::exchanges::{
    events::AllowedEventSourceType, general::commission::Commission,
    timeouts::timeout_manager::TimeoutManager,
};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::binance::binance::*;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::orders::event::OrderEventType;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;

use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::events::ExchangeEvent;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use tokio::sync::broadcast;

#[actix_rt::test]
async fn create_successfully() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, mut rx) = broadcast::channel(10);

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
        "FromCreateOrderTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: dec!(0.0000001),
    };

    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    let created_order = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await;

    match created_order {
        Ok(order_ref) => {
            let event = rx
                .recv()
                .await
                .expect("CreateOrderSucceeded event had to be occurred");
            let order_event = if let ExchangeEvent::OrderEvent(order_event) = event {
                order_event
            } else {
                panic!("Should receive OrderEvent")
            };

            if let OrderEventType::CreateOrderSucceeded = order_event.event_type {
            } else {
                panic!("Should receive CreateOrderSucceeded event type")
            }

            let exchange_order_id = order_ref.exchange_order_id().expect("in test");
            let order_to_cancel = OrderCancelling {
                header: order_header,
                exchange_order_id,
            };

            // Cancel last order
            let _cancel_outcome = exchange
                .cancel_order(&order_to_cancel, CancellationToken::default())
                .await;
        }

        // Create order failed
        Err(_) => {
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn should_fail() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, _rx) = broadcast::channel(10);

    settings.web_socket_host = "https://host.com".into();
    settings.web_socket2_host = "https://host2.com".into();
    let binance = Binance::new(
        exchange_account_id,
        settings,
        tx.clone(),
        application_manager.clone(),
    );

    let exchange = Exchange::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
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

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
        Utc::now(),
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(1),
        OrderExecutionType::None,
        None,
        None,
        "FromCreateOrderTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header,
        price: dec!(0.0000001),
    };

    let created_order = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await;

    match created_order {
        Ok(_) => {
            assert!(false)
        }
        Err(error) => {
            assert_eq!(
                "Delete it in the future. Exchange error: Unable to send POST request: client requires absolute-form URIs",
                error.to_string()
            );
        }
    }
}
