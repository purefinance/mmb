use crate::bitmex::bitmex_builder::BitmexBuilder;
use core_tests::order::OrderProxy;
use mmb_domain::events::{AllowedEventSourceType, ExchangeEvent};
use mmb_domain::order::event::OrderEventType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rstest::rstest;
use rust_decimal_macros::dec;
use std::time::Duration;

#[rstest]
#[case::all(AllowedEventSourceType::All)]
#[case::fallback_only(AllowedEventSourceType::FallbackOnly)]
#[case::non_fallback(AllowedEventSourceType::NonFallback)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_successfully(#[case] allowed_create_event_source_type: AllowedEventSourceType) {
    init_logger_file_named("log.txt");

    let mut bitmex_builder = match BitmexBuilder::build_account_with_source_types(
        allowed_create_event_source_type,
        AllowedEventSourceType::default(),
        true,
    )
    .await
    {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.execution_price,
        bitmex_builder.min_amount,
        bitmex_builder.default_currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);

    let order_ref = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    let event = bitmex_builder
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
        .cancel_order_or_fail(&order_ref, bitmex_builder.exchange)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn should_fail() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(true).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromShouldFailTest".to_owned()),
        CancellationToken::default(),
        dec!(0.0000000000000000001),
        dec!(1),
        bitmex_builder.default_currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);

    let error = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect_err("should be error");

    assert!(error.to_string().contains("Invalid price"));
}
