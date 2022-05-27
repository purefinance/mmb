use mmb_core::orders::event::OrderEventType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::*;

use mmb_core::exchanges::events::ExchangeEvent;

use crate::binance::binance_builder::BinanceBuilder;
use core_tests::order::OrderProxy;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_successfully() {
    init_logger_file_named("log.txt");

    let mut binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };
    let exchange_account_id = binance_builder.exchange.exchange_account_id;

    let order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
        binance_builder.min_amount,
    );

    let order_ref = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    let event = binance_builder
        .rx
        .recv()
        .await
        .expect("CreateOrderSucceeded event had to be occurred");

    let order_event = if let ExchangeEvent::OrderEvent(order_event) = event {
        order_event
    } else {
        panic!("Should receive OrderEvent")
    };

    match order_event.event_type {
        OrderEventType::CreateOrderSucceeded => {}
        _ => panic!("Should receive CreateOrderSucceeded event type"),
    }

    order_proxy
        .cancel_order_or_fail(&order_ref, binance_builder.exchange.clone())
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn should_fail() {
    init_logger_file_named("log.txt");

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };
    let exchange_account_id = binance_builder.exchange.exchange_account_id;

    let mut order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromShouldFailTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
        binance_builder.min_amount,
    );
    order_proxy.amount = dec!(1);
    order_proxy.price = dec!(0.0000000000000000001);

    match order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
    {
        Ok(error) => assert!(false, "Create order failed with error {:?}.", error),
        Err(error) => assert_eq!(
            "Exchange error: Type: InvalidOrder Message: Precision is over the maximum defined for this asset. Code Some(-1111)",
            error.to_string()
        ),
    }
}
