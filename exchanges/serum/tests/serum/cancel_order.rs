use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::snapshot::OrderSide;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::init_infrastructure;
use rust_decimal_macros::dec;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_successfully() {
    let serum_builder = SerumBuilder::build_account_0().await;
    let exchange_account_id = serum_builder.exchange.exchange_account_id;

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
    init_infrastructure();

    let serum_builder = SerumBuilder::build_account_0().await;
    let exchange_account_id = serum_builder.exchange.exchange_account_id;

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
