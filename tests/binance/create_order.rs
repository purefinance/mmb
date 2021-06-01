use crate::get_binance_credentials_or_exit;
use chrono::Utc;
use mmb::exchanges::{events::AllowedEventSourceType, general::commission::Commission};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::binance::binance::*;
use mmb_lib::core::exchanges::cancellation_token::CancellationToken;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::sync::mpsc::channel;
use std::{env, sync::Arc};

#[actix_rt::test]
async fn create_successfully() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let settings = settings::ExchangeSettings::new(
        exchange_account_id.clone(),
        api_key.expect("in test"),
        secret_key.expect("in test"),
        false,
    );

    let binance = Binance::new(settings, exchange_account_id.clone());

    let websocket_host = "wss://stream.binance.com:9443".into();
    let currency_pairs = vec!["PHBBTC".into()];
    let channels = vec!["depth".into(), "trade".into()];

    let (tx, rx) = channel();
    let exchange = Exchange::new(
        exchange_account_id.clone(),
        websocket_host,
        currency_pairs,
        channels,
        Box::new(binance),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        Commission::default(),
    );

    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
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
                .expect("CreateOrderSucceeded event had to be occured");
            if event.event_type != OrderEventType::CreateOrderSucceeded {
                assert!(false)
            }

            let exchange_order_id = order_ref.exchange_order_id().expect("in test");
            let order_to_cancel = OrderCancelling {
                header: Arc::new(order_header),
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

    let settings = settings::ExchangeSettings::new(
        exchange_account_id.clone(),
        api_key.expect("in test"),
        secret_key.expect("in test"),
        false,
    );

    let binance = Binance::new(settings, exchange_account_id);

    let (tx, _rx) = channel();
    let exchange = Exchange::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        "host".into(),
        vec![],
        vec![],
        Box::new(binance),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        Commission::default(),
    );

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
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
                "Delete it in the future. Exchange error: Filter failure: MIN_NOTIONAL",
                error.to_string()
            );
        }
    }
}
