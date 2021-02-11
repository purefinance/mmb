use chrono::Utc;
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::actor::*;
use mmb_lib::core::exchanges::binance::*;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::env;

#[actix_rt::test]
async fn test_add() {
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

    let mut exchange_actor = ExchangeActor::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        "host".into(),
        vec![],
        vec![],
        Box::new(binance),
    );

    let test_currency_pair = CurrencyPair::new("TNBBTC".into());
    let order_header = OrderHeader::new(
        "test".into(),
        Utc::now(),
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
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
        header: order_header,
        // It has to be between (current price on exchange * 0.2) and (current price on exchange * 5)
        price: dec!(0.00000002),
    };

    let create_order_result = exchange_actor.create_order(&order_to_create).await;

    match create_order_result.outcome {
        RequestResult::Success(order_id) => {
            let order_to_cancel = OrderCancelling {
                currency_pair: test_currency_pair,
                order_id,
            };

            // Cancel last order
            let _cancel_outcome = exchange_actor.cancel_order(&order_to_cancel).await;
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

    let mut exchange_actor = ExchangeActor::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        "host".into(),
        vec![],
        vec![],
        Box::new(binance),
    );

    let test_currency_pair = CurrencyPair::new("TNBBTC".into());
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

    let create_order_result = exchange_actor.create_order(&order_to_create).await;

    let expected_error = RequestResult::Error(ExchangeError::new(
        ExchangeErrorType::InvalidOrder,
        "Filter failure: MIN_NOTIONAL".to_owned(),
        Some(-1013),
    ));

    // It's MIN_NOTIONAL error code
    assert_eq!(create_order_result.outcome, expected_error);
}
