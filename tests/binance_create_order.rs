use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::actor::*;
use mmb_lib::core::exchanges::binance::*;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::env;
use tokio;

#[tokio::test]
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

    let order = DataToCreateOrder {
        side: OrderSide::Buy,
        order_type: OrderType::Limit,
        // It have to be between (current price on exchange * 0.2) and (current price on exchange * 5)
        price: dec!(0.01),
        execution_type: OrderExecutionType::None,
        currency_pair: CurrencyPair::new("ETHBTC".into()),
        client_order_id: "test".into(),
        amount: dec!(1),
    };

    let create_order = exchange_actor.create_order(&order);

    tokio::join!(create_order);

    assert!(true);
}
