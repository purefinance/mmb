use crate::serum::common::retry_action;
use crate::serum::serum_builder::SerumBuilder;
use anyhow::anyhow;
use core_tests::order::OrderProxyBuilder;
use mmb_core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_core::exchanges::events::AllowedEventSourceType;
use mmb_core::exchanges::general::commission::Commission;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::orders::order::{ClientOrderId, OrderSide};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::dec;
use std::collections::BTreeSet;
use std::time::Duration;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_open_orders() {
    init_logger_file_named("log.txt");

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
        Some("FromGetOpenOrdersTest".to_owned()),
        dec!(1),
        dec!(1),
    )
    .currency_pair(currency_pair)
    .side(OrderSide::Sell)
    .timeout(Duration::from_secs(30))
    .build();

    first_order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create first order failed");

    let second_order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        dec!(2),
        dec!(1),
    )
    .currency_pair(currency_pair)
    .side(OrderSide::Sell)
    .timeout(Duration::from_secs(30))
    .build();

    second_order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create second order failed");

    let all_orders = retry_action(10, Duration::from_secs(2), "Get open orders", || async {
        serum_builder
            .exchange
            .get_open_orders(false)
            .await
            .and_then(|orders| {
                if orders.len() == 2 {
                    Ok(orders)
                } else {
                    Err(anyhow!("Incorrect orders len: {}", orders.len()))
                }
            })
    })
    .await
    .expect("Failed to get open orders");

    serum_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    assert_eq!(all_orders.len(), 2);
}

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_open_orders_for_currency_pair() {
    init_logger_file_named("log.txt");

    let exchange_account_id: ExchangeAccountId = "Serum_0".parse().expect("Parsing error");
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

    let first_currency_pair = CurrencyPair::from_codes("sol".into(), "test".into());
    let first_order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        dec!(1),
        dec!(1),
    )
    .currency_pair(first_currency_pair)
    .side(OrderSide::Sell)
    .timeout(Duration::from_secs(30))
    .build();

    let mut expected_orders_id = BTreeSet::new();

    first_order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create first order failed");
    expected_orders_id.insert(first_order_proxy.client_order_id);

    let second_currency_pair = CurrencyPair::from_codes("sol".into(), "test".into());
    let second_order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        dec!(2),
        dec!(1),
    )
    .currency_pair(second_currency_pair)
    .side(OrderSide::Sell)
    .timeout(Duration::from_secs(30))
    .build();

    second_order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create second order failed");
    expected_orders_id.insert(second_order_proxy.client_order_id);

    let all_orders = retry_action(10, Duration::from_secs(2), "Get open orders", || async {
        serum_builder
            .exchange
            .get_open_orders(false)
            .await
            .and_then(|orders| {
                if 2 <= orders.len() {
                    Ok(orders)
                } else {
                    Err(anyhow!("Incorrect orders len: {}", orders.len()))
                }
            })
    })
    .await
    .expect("Failed to get open orders");

    serum_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    let orders_id: BTreeSet<ClientOrderId> =
        all_orders.into_iter().map(|x| x.client_order_id).collect();

    assert_eq!(expected_orders_id, orders_id);
}
