use chrono::Utc;
use mmb_lib::core::exchanges::binance::binance::*;
use mmb_lib::core::exchanges::cancellation_token::CancellationToken;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::exchange::*;
use mmb_lib::core::exchanges::exchange_features::*;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::env;

#[actix_rt::test]
async fn cancelled_successfully() {
    // Get data to access binance account
    let api_key = env::var("BINANCE_API_KEY");
    if api_key.is_err() {
        dbg!("Environment variable BINANCE_API_KEY are not set. Unable to continue test");
        return;
    }

    let secret_key = env::var("BINANCE_SECRET_KEY");
    if secret_key.is_err() {
        dbg!("Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test");
        return;
    }

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
        ExchangeFeatures::new(OpenOrdersType::AllCurrencyPair, false),
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

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        // It has to be between (current price on exchange * 0.2) and (current price on exchange * 5)
        price: dec!(0.0000001),
    };

    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    let create_order_result = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await
        .expect("in test");
    dbg!(&create_order_result);

    match create_order_result.outcome {
        RequestResult::Success(exchange_order_id) => {
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
                assert_eq!(gotten_client_order_id, generated_client_order_id);
                return;
            }
        }

        // Create order failed
        RequestResult::Error(_) => {
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn nothing_to_cancel() {
    // Get data to access binance account
    let api_key = env::var("BINANCE_API_KEY");
    if api_key.is_err() {
        dbg!("Environment variable BINANCE_API_KEY are not set. Unable to continue test");
        return;
    }

    let secret_key = env::var("BINANCE_SECRET_KEY");
    if secret_key.is_err() {
        dbg!("Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test");
        return;
    }

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
        ExchangeFeatures::new(OpenOrdersType::AllCurrencyPair, false),
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
