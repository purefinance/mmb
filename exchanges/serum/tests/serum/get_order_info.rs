use crate::serum::common::retry_action;
use crate::serum::serum_builder::SerumBuilder;
use anyhow::anyhow;
use core_tests::order::OrderProxyBuilder;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::orders::order::OrderSide;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::dec;
use std::time::Duration;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_order_info() {
    init_logger_file_named("log.txt");

    let serum_builder = SerumBuilder::build_account_0().await;
    let exchange_account_id = serum_builder.exchange.exchange_account_id;

    let currency_pair = CurrencyPair::from_codes("sol".into(), "test".into());
    let order_proxy = OrderProxyBuilder::new(
        exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        dec!(1),
        dec!(1),
    )
    .currency_pair(currency_pair)
    .side(OrderSide::Sell)
    .build();

    let created_order = order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create order failed");

    let gotten_info_exchange_order_id = retry_action(
        10,
        Duration::from_secs(2),
        "Get exchange order id",
        || async {
            serum_builder
                .exchange
                .get_order_info(&created_order)
                .await
                .map(|order_info| order_info.exchange_order_id)
                .map_err(|err| anyhow!("{err:?}"))
        },
    )
    .await
    .expect("Failed to get order info");

    let created_exchange_order_id = created_order
        .exchange_order_id()
        .expect("Cannot get exchange_order_id");

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
