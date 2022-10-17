use crate::binance::binance_builder::BinanceBuilder;
use crate::binance::common::default_currency_pair;
use core_tests::order::OrderProxy;
use mmb_domain::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_domain::order::event::OrderEventType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rstest::rstest;
use rust_decimal_macros::*;
use std::time::Duration;

#[rstest]
#[case::all(AllowedEventSourceType::All)]
#[case::fallback_only(AllowedEventSourceType::FallbackOnly)]
#[case::non_fallback(AllowedEventSourceType::NonFallback)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_successfully(#[case] allowed_create_event_source_type: AllowedEventSourceType) {
    init_logger_file_named("log.txt");

    let binance_builder = BinanceBuilder::build_account_0_with_source_types(
        allowed_create_event_source_type,
        AllowedEventSourceType::default(),
    );

    let mut binance_builder = match binance_builder.await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };
    let exchange_account_id = binance_builder.exchange.exchange_account_id;

    let mut order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
        binance_builder.min_amount,
        default_currency_pair(),
    );
    order_proxy.timeout = Duration::from_secs(15);

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

    let order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromShouldFailTest".to_owned()),
        CancellationToken::default(),
        dec!(0.0000000000000000001),
        dec!(1),
        default_currency_pair(),
    );

    let error = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect_err("should be error");

    assert_eq!(
        "failed create_order: Precision is over the maximum defined for this asset.",
        error.to_string()
    );
}
