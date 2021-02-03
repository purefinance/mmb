use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::actor::*;
use mmb_lib::core::exchanges::binance::*;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::env;

// TODO Why does it don't work with tokio test?
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

    let binance = Binance::new(settings, "some_id".into());

    let exchange_actor = ExchangeActor::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        "host".into(),
        vec![],
        vec![],
        Box::new(binance),
    );

    let order_to_create = DataToCreateOrder {
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        // It have to be between (current price on exchange * 0.2) and (current price on exchange * 5)
        price: dec!(0.0000001),
        execution_type: OrderExecutionType::None,
        currency_pair: CurrencyPair::new("DENTETH".into()),
        client_order_id: "test".into(),
        amount: dec!(100000),
    };

    //exchange_actor.create_order(&order_to_create).await;
    //exchange_actor.get_account_info().await;
    //exchange_actor.cancel_all_orders().await;

    let order_to_cancel = DataToCancelOrder {
        currency_pair: CurrencyPair::new("DENTETH".into()),
        order_id: "30".into(),
    };

    exchange_actor.cancel_order(&order_to_cancel).await;

    assert!(true);
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

    let binance = Binance::new(settings, "some_id".into());

    let exchange_actor = ExchangeActor::new(
        mmb::exchanges::common::ExchangeAccountId::new("".into(), 0),
        "host".into(),
        vec![],
        vec![],
        Box::new(binance),
    );

    let order_to_create = DataToCreateOrder {
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        // It have to be between (current price on exchange * 0.2) and (current price on exchange * 5)
        price: dec!(0.0000001),
        execution_type: OrderExecutionType::None,
        currency_pair: CurrencyPair::new("DENTETH".into()),
        client_order_id: "test".into(),
        amount: dec!(100000),
    };

    let create_order_result = exchange_actor.create_order(&order_to_create).await;
    dbg!(&create_order_result);

    assert!(true);
}
