use crate::binance::binance_builder::BinanceBuilder;
use core_tests::order::OrderProxy;
use mmb_domain::events::AllowedEventSourceType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger;
use rstest::rstest;

#[rstest]
#[case::all(AllowedEventSourceType::All)]
#[case::fallback_only(AllowedEventSourceType::FallbackOnly)]
#[case::non_fallback(AllowedEventSourceType::NonFallback)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_waited_successfully(
    #[case] allowed_cancel_event_source_type: AllowedEventSourceType,
) {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0_with_source_types(
        AllowedEventSourceType::All,
        allowed_cancel_event_source_type,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        binance_builder.exchange.exchange_account_id,
        Some("FromCancellationWaitedSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
    );

    let order_ref = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    // If here are no error - order was cancelled successfully
    binance_builder
        .exchange
        .wait_cancel_order(order_ref, None, true, CancellationToken::new())
        .await
        .expect("Error while trying wait_cancel_order");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_waited_failed_fallback() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0_with_source_types(
        AllowedEventSourceType::All,
        AllowedEventSourceType::FallbackOnly,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        binance_builder.exchange.exchange_account_id,
        Some("FromCancellationWaitedFailedFallbackTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
    );

    let order_ref = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    let error = binance_builder
        .exchange
        .wait_cancel_order(order_ref, None, true, CancellationToken::new())
        .await
        .expect_err("Error was expected while trying wait_cancel_order()");

    assert_eq!(
        "Order was expected to cancel explicitly via Rest or Web Socket but got timeout instead",
        &error.to_string()[..86]
    );
}
