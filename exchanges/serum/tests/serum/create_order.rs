use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_domain::events::ExchangeEvent;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::event::{OrderEvent, OrderEventType};
use mmb_domain::order::snapshot::OrderSide;
use mmb_utils::nothing_to_do;
use rust_decimal_macros::dec;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_successfully() {
    let mut serum_builder = SerumBuilder::build_account_0().await;
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
    .timeout(Duration::from_secs(30))
    .build();

    let order_ref = order_proxy
        .create_order(serum_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    check_exchange_order_event_is_succeed_or_panic(&mut serum_builder.rx).await;

    order_proxy
        .cancel_order_or_fail(&order_ref, serum_builder.exchange.clone())
        .await;
}

async fn receive_exchange_order_event(
    receiver: &mut broadcast::Receiver<ExchangeEvent>,
) -> OrderEvent {
    // we can get another event first
    for attempt in 0..3 {
        let event = receiver.recv().await.expect("Failed to get exchange event");
        if let ExchangeEvent::OrderEvent(order_event) = event {
            return order_event;
        }

        log::warn!("Should receive OrderEvent {:?}. Attempt {}", event, attempt);
    }

    panic!("Should receive OrderEvent")
}

async fn check_exchange_order_event_is_succeed_or_panic(
    receiver: &mut broadcast::Receiver<ExchangeEvent>,
) {
    let receive_fut = receive_exchange_order_event(receiver);
    let order_event = timeout(Duration::from_secs(2), receive_fut)
        .await
        .expect("Receiver exchange order event time is gone");

    match order_event.event_type {
        OrderEventType::CreateOrderSucceeded => nothing_to_do(),
        _ => panic!("Should receive CreateOrderSucceeded event type"),
    }
}
