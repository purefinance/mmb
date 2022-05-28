use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_core::exchanges::common::CurrencyPair;
use mmb_core::exchanges::events::ExchangeEvent;
use mmb_core::orders::event::OrderEventType;
use mmb_core::orders::order::{OrderSide, OrderSnapshot};
use mmb_utils::infrastructure::init_infrastructure;
use rust_decimal_macros::dec;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn order_fill() {
    init_infrastructure("log.txt");

    let first_serum_builder = SerumBuilder::build_account_0().await;
    let first_exchange_account_id = first_serum_builder.exchange.exchange_account_id;
    let price = dec!(1);
    let first_amount = dec!(10);
    let first_order_proxy = OrderProxyBuilder::new(
        first_exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        price,
        first_amount,
    )
    .currency_pair(CurrencyPair::from_codes("sol".into(), "test".into()))
    .side(OrderSide::Buy)
    .timeout(Duration::from_secs(30))
    .build();

    let mut second_serum_builder = SerumBuilder::build_account_1().await;
    let second_exchange_account_id = second_serum_builder.exchange.exchange_account_id;
    let second_amount = dec!(13);
    let second_order_proxy = OrderProxyBuilder::new(
        second_exchange_account_id,
        Some("FromCreateSuccessfullyTest".to_owned()),
        price,
        second_amount,
    )
    .currency_pair(CurrencyPair::from_codes("sol".into(), "test".into()))
    .side(OrderSide::Sell)
    .timeout(Duration::from_secs(30))
    .build();

    let first_order_ref = first_order_proxy
        .create_order(first_serum_builder.exchange.clone())
        .await
        .expect("Create first order failed with error");

    let second_order_ref = second_order_proxy
        .create_order(second_serum_builder.exchange.clone())
        .await
        .expect("Create second order failed with error");

    let order_snapshot = receive_exchange_order_event(&mut second_serum_builder.rx).await;

    let expected_order_fill_amount = second_amount - first_amount;
    assert_eq!(expected_order_fill_amount, order_snapshot.filled_amount());

    first_order_proxy
        .cancel_order_or_fail(&first_order_ref, first_serum_builder.exchange.clone())
        .await;

    second_order_proxy
        .cancel_order_or_fail(&second_order_ref, second_serum_builder.exchange.clone())
        .await;
}

async fn receive_exchange_order_event(
    receiver: &mut broadcast::Receiver<ExchangeEvent>,
) -> Arc<OrderSnapshot> {
    // we can get another event first
    for attempt in 0..10 {
        let event = receiver.recv().await.expect("Failed to get exchange event");
        if let ExchangeEvent::OrderEvent(order_event) = &event {
            if let OrderEventType::OrderFilled {
                cloned_order: order,
            } = &order_event.event_type
            {
                return order.clone();
            }

            log::warn!(
                "Should receive OrderFilled event {:#?}. Attempt {}",
                order_event,
                attempt
            );
        }
    }

    panic!("Should receive OrderEvent")
}
