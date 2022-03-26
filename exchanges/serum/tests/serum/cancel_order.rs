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

    let currency_pair = CurrencyPair::from_codes("sol".into(), "test2".into());
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
