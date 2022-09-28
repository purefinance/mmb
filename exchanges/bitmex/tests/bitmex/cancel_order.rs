use crate::bitmex::bitmex_builder::BitmexBuilder;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::snapshot::{OrderCancelling, OrderSide};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::dec;
use std::time::Duration;

// TODO Check this test again after websocket messages handling implementation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_successfully() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(false).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromCancelledSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        dec!(10000),
        dec!(100),
    );
    order_proxy.timeout = Duration::from_secs(15);
    order_proxy.currency_pair = CurrencyPair::from_codes("xbt".into(), "usd".into());
    order_proxy.side = OrderSide::Buy;

    let order_ref = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order failed with error:");

    order_proxy
        .cancel_order_or_fail(&order_ref, bitmex_builder.exchange.clone())
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_all() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(false).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromCancelledSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        dec!(10000),
        dec!(100),
    );
    order_proxy.timeout = Duration::from_secs(15);
    order_proxy.currency_pair = CurrencyPair::from_codes("xbt".into(), "usd".into());
    order_proxy.side = OrderSide::Buy;

    order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order failed with error:");

    bitmex_builder
        .exchange
        .cancel_all_orders(order_proxy.currency_pair)
        .await
        .expect("Failed to cancel all orders");

    let orders = &bitmex_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Opened orders not found for exchange account id");

    assert_eq!(orders.len(), 0);
}

// TODO Check this test again after websocket messages handling implementation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_to_cancel() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(false).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromNothingToCancelTest".to_owned()),
        CancellationToken::default(),
        dec!(10000),
        dec!(100),
    );
    order_proxy.timeout = Duration::from_secs(15);
    order_proxy.currency_pair = CurrencyPair::from_codes("xbt".into(), "usd".into());
    order_proxy.side = OrderSide::Buy;

    let order_to_cancel = OrderCancelling {
        header: order_proxy.make_header(),
        exchange_order_id: "1234567890".into(),
        extension_data: None,
    };

    // Cancel last order
    let cancel_outcome = bitmex_builder
        .exchange
        .cancel_order(order_to_cancel, CancellationToken::default())
        .await
        .expect("in test");
    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(
            error.message,
            "Type: OrderNotFound Message: Unknown order sent. Code Some(-2011)"
        );
    }
}
