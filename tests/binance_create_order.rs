use actix::{Actor, Addr, Arbiter, Context, System};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::actor::*;
use mmb_lib::core::exchanges::binance::*;
use mmb_lib::core::exchanges::common_interaction::*;
use mmb_lib::core::settings;
use std::env;

#[test]
fn test_add() {
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

    let mut system = System::new("test");

    let addr = system.block_on(exchange_actor.create_order());

    system.run();

    assert!(true)
}
