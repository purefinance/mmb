use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;

use crate::binance::binance_builder::BinanceBuilder;
use crate::core::order::OrderProxy;

#[actix_rt::test]
async fn cancelled_successfully() {
    init_logger();

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
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
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
        Some("FromCancelledSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
    );

    let order_ref = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("Create order failed with error:");

    order_proxy
        .cancel_order_or_fail(&order_ref, binance_builder.exchange.clone())
        .await;
}

#[actix_rt::test]
async fn cancel_opened_orders_successfully() {
    init_logger();

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
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let first_order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCancelOpenedOrdersSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
    );
    first_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let second_order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromCancelOpenedOrdersSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
    );
    second_order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let orders = &binance_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Opened orders not found for exchange account id:");

    assert_ne!(orders.len(), 0);
    binance_builder
        .exchange
        .clone()
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    let orders = &binance_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Opened orders not found for exchange account id");

    assert_eq!(orders.len(), 0);
}

#[actix_rt::test]
async fn nothing_to_cancel() {
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
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order = OrderProxy::new(
        exchange_account_id,
        Some("FromNothingToCancelTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
    );
    let order_to_cancel = OrderCancelling {
        header: order.make_header(),
        exchange_order_id: "1234567890".into(),
    };

    // Cancel last order
    let cancel_outcome = binance_builder
        .exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test")
        .expect("in test");
    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
