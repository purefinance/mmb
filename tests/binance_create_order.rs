use chrono::Utc;
use mmb_lib::core as mmb;
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
async fn create_successfully() {
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

    let settings = settings::ExchangeSettings {
        api_key: api_key.unwrap(),
        secret_key: secret_key.unwrap(),
        is_marging_trading: false,
        web_socket_host: "".into(),
        web_socket2_host: "".into(),
        rest_host: "https://api.binance.com".into(),
    };

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().unwrap();
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
    let order_header = OrderHeader::new(
        ClientOrderId::unique_id(),
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
        price: dec!(0.00000004),
    };

    let _ = exchange.cancel_all_orders(test_currency_pair.clone()).await;
    let create_order_result = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await
        .unwrap();

    match create_order_result.outcome {
        RequestResult::Success(exchange_order_id) => {
            let order_to_cancel = OrderCancelling {
                header: order_header,
                exchange_order_id,
            };

            // Cancel last order
            let cancel_outcome = exchange
                .cancel_order(&order_to_cancel, CancellationToken::default())
                .await;
            dbg!(&cancel_outcome);
        }

        // Create order failed
        RequestResult::Error(_) => {
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn should_fail() {
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

    let settings = settings::ExchangeSettings {
        api_key: api_key.unwrap(),
        secret_key: secret_key.unwrap(),
        is_marging_trading: false,
        web_socket_host: "".into(),
        web_socket2_host: "".into(),
        rest_host: "https://api.binance.com".into(),
    };

    let binance = Binance::new(settings, "Binance0".parse().unwrap());

    let exchange = Exchange::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        "host".into(),
        vec![],
        vec![],
        Box::new(binance),
        ExchangeFeatures::new(OpenOrdersType::AllCurrencyPair, false),
    );

    let test_currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
    let order_header = OrderHeader::new(
        "test".into(),
        Utc::now(),
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(1),
        OrderExecutionType::None,
        ReservationId::gen_new(),
        None,
        "".into(),
    );

    let order_to_create = OrderCreating {
        header: order_header,
        // It have to be between (current price on exchange * 0.2) and (current price on exchange * 5)
        price: dec!(0.00000005),
    };

    let create_order_result = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await
        .unwrap();

    let expected_error = RequestResult::Error(ExchangeError::new(
        ExchangeErrorType::InvalidOrder,
        "Filter failure: MIN_NOTIONAL".to_owned(),
        Some(-1013),
    ));

    // It's MIN_NOTIONAL error code
    assert_eq!(create_order_result.outcome, expected_error);
}
