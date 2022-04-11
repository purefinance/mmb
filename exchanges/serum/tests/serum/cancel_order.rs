use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_core::exchanges::events::AllowedEventSourceType;
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::orders::order::OrderSide;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::init_infrastructure;
use rust_decimal_macros::dec;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_successfully() {
    let exchange_account_id = ExchangeAccountId::new("Serum".into(), 0);
    let serum_builder = SerumBuilder::try_new(
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
    )
    .await
    .expect("Failed to create SerumBuilder");

    let currency_pair = CurrencyPair::from_codes("sol".into(), "test".into());
    let order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        dec!(1),
        dec!(1),
    )
    .currency_pair(currency_pair)
    .side(OrderSide::Sell)
    .build();

    let order_ref = order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create order failed with error:");

    order_proxy
        .cancel_order_or_fail(&order_ref, serum_builder.exchange.clone())
        .await;
}

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_all_orders() {
    init_infrastructure("log.txt");

    let exchange_account_id = ExchangeAccountId::new("Serum".into(), 0);
    let serum_builder = SerumBuilder::try_new(
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
    )
    .await
    .expect("Failed to create SerumBuilder");

    let currency_pair = CurrencyPair::from_codes("sol".into(), "test".into());
    let first_order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        dec!(1),
        dec!(1),
    )
    .currency_pair(currency_pair)
    .side(OrderSide::Sell)
    .build();

    first_order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create first order failed with error:");

    let second_order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        dec!(2),
        dec!(10),
    )
    .currency_pair(currency_pair)
    .side(OrderSide::Sell)
    .build();

    second_order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create second order failed with error:");

    serum_builder
        .exchange
        .clone()
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    let all_orders = serum_builder
        .exchange
        .clone()
        .get_open_orders(false)
        .await
        .expect("Get open orders failed");

    assert_eq!(all_orders.len(), 0);
}
