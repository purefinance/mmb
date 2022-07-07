use core_tests::order::OrderProxy;
use mmb_core::exchanges::common::ExchangeAccountId;
use mmb_core::exchanges::events::AllowedEventSourceType;
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;

use crate::binance::binance_builder::BinanceBuilder;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_waited_successfully() {
    init_logger_file_named("log.txt");

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };
    let exchange_account_id = binance_builder.exchange.exchange_account_id;

    let order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCancellationWaitedSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
        binance_builder.min_amount,
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
    init_logger_file_named("log.txt");

    let exchange_account_id: ExchangeAccountId = "Binance_0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
        exchange_account_id,
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::FallbackOnly,
        ),
        Commission::default(),
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCancellationWaitedFailedFallbackTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
        binance_builder.min_amount,
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
