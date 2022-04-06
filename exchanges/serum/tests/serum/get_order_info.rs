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
use mmb_core::orders::order::OrderSide;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::dec;
use std::time::Duration;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_order_info() {
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
