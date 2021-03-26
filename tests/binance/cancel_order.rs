use crate::get_binance_credentials;
use chrono::Utc;
use mmb_lib::core::exchanges::binance::binance::*;
use mmb_lib::core::exchanges::cancellation_token::CancellationToken;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::env;

#[actix_rt::test]
async fn cancelled_successfully() {
    let (api_key, secret_key) = get_binance_credentials!();

    let settings = settings::ExchangeSettings::new(
        api_key.expect("in test"),
        secret_key.expect("in test"),
        false,
    );

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let binance = Binance::new(settings, exchange_account_id.clone());

    let websocket_host = "wss://stream.binance.com:9443".into();
    let currency_pairs = vec!["PHBBTC".into()];
    let channels = vec!["depth".into(), "trade".into()];

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        websocket_host,
        currency_pairs,
        channels,
        Box::new(binance),
        ExchangeFeatures::new(OpenOrdersType::AllCurrencyPair, false, true),
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
        ReservationId::gen_new(),
        None,
        "".into(),
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
            let exchange_order_id = order_ref.exchange_order_id().expect("in test");
            let order_to_cancel = OrderCancelling {
                header: order_header,
                exchange_order_id,
            };

            // Cancel last order
            let cancel_outcome = exchange
                .cancel_order(&order_to_cancel, CancellationToken::default())
                .await
                .expect("in test");

            if let RequestResult::Success(gotten_client_order_id) = cancel_outcome.outcome {
                assert_eq!(gotten_client_order_id, test_order_client_id);
            }
        }

        // Create order failed
        Err(_) => {
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn nothing_to_cancel() {
    let (api_key, secret_key) = get_binance_credentials!();

    let settings = settings::ExchangeSettings::new(
        api_key.expect("in test"),
        secret_key.expect("in test"),
        false,
    );

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let binance = Binance::new(settings, exchange_account_id.clone());

    let websocket_host = "wss://stream.binance.com:9443".into();
    let currency_pairs = vec!["PHBBTC".into()];
    let channels = vec!["depth".into(), "trade".into()];

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        websocket_host,
        currency_pairs,
        channels,
        Box::new(binance),
        ExchangeFeatures::new(OpenOrdersType::AllCurrencyPair, false, true),
    );

    exchange.clone().connect().await;

    let test_currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
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
        ReservationId::gen_new(),
        None,
        "".into(),
    );

    let order_to_cancel = OrderCancelling {
        header: order_header,
        exchange_order_id: "1234567890".into(),
    };

    // Cancel last order
    let cancel_outcome = exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test");

    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
