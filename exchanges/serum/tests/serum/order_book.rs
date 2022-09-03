use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_domain::events::ExchangeEvent;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::snapshot::OrderSide;
use mmb_domain::order_book_data;
use rust_decimal_macros::dec;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fill_order_book() {
    let serum_builder_0 = SerumBuilder::build_account_0().await;
    let exchange_account_id_0 = serum_builder_0.exchange.exchange_account_id;

    let price1 = dec!(1);
    let amount1 = dec!(1);
    let order_proxy1 = OrderProxyBuilder::new(
        exchange_account_id_0,
        Some("FromCreateSuccessfullyTest".to_owned()),
        price1,
        amount1,
    )
    .currency_pair(CurrencyPair::from_codes("sol".into(), "test".into()))
    .side(OrderSide::Buy)
    .build();

    let mut serum_builder_1 = SerumBuilder::build_account_1().await;
    let exchange_account_id_1 = serum_builder_1.exchange.exchange_account_id;

    let price2 = dec!(2);
    let amount2 = dec!(10);
    let order_proxy2 = OrderProxyBuilder::new(
        exchange_account_id_1,
        Some("FromCreateSuccessfullyTest".to_owned()),
        price2,
        amount2,
    )
    .currency_pair(CurrencyPair::from_codes("sol".into(), "test".into()))
    .side(OrderSide::Buy)
    .build();

    let order_ref1 = order_proxy1
        .create_order(serum_builder_0.exchange.clone())
        .await
        .expect("Create order1 failed with error");

    let order_ref2 = order_proxy2
        .create_order(serum_builder_1.exchange.clone())
        .await
        .expect("Create order2 failed with error");

    let event = serum_builder_1
        .rx
        .recv()
        .await
        .expect("CreateOrderSucceeded event had to be occurred");

    let order_book_event = if let ExchangeEvent::OrderBookEvent(order_book_event) = event {
        order_book_event
    } else {
        panic!("Should receive OrderBookEvent")
    };

    let expected_order_book_data = order_book_data![
        ;
        price1 => amount1,
        price2 => amount2,
    ];

    assert_eq!(expected_order_book_data, *order_book_event.data);

    order_proxy1
        .cancel_order_or_fail(&order_ref1, serum_builder_0.exchange.clone())
        .await;

    order_proxy2
        .cancel_order_or_fail(&order_ref2, serum_builder_1.exchange.clone())
        .await;
}
