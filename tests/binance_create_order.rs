use actix::{Actor, Addr, Arbiter, Context, System};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::actor::*;
use mmb_lib::core::exchanges::binance::*;
use mmb_lib::core::exchanges::common_interaction::*;

#[test]
fn test_add() {
    let binance = Binance {
        id: "binance_instance_for_test".to_owned(),
    };

    let exchange_actor = ExchangeActor::new(
        mmb::exchanges::common::ExchangeId::new("".into(), 0),
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
