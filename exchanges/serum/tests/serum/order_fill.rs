use crate::serum::serum_builder::SerumBuilder;
use core_tests::order::OrderProxyBuilder;
use mmb_domain::events::ExchangeEvent;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::event::OrderEventType;
use mmb_domain::order::snapshot::{ClientOrderId, OrderSide, OrderSnapshot};
use mmb_utils::infrastructure::init_infrastructure;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use scopeguard::defer;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn partial_order_fill() {
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
    let first_builder_exchange = first_serum_builder.exchange.clone();
    defer! {
        tokio::spawn(async move {
            first_order_proxy
                .cancel_order_or_fail(&first_order_ref, first_builder_exchange.clone())
                .await;
        });
    }

    let second_order_ref = second_order_proxy
        .create_order(second_serum_builder.exchange.clone())
        .await
        .expect("Create second order failed with error");
    let second_client_order_id = second_order_ref.client_order_id();
    let second_builder_exchange = second_serum_builder.exchange.clone();
    defer! {
        tokio::spawn(async move {
            second_order_proxy
                .cancel_order_or_fail(&second_order_ref, second_builder_exchange.clone())
                .await;
        });
    }

    let order_snapshot =
        receive_exchange_order_event(&mut second_serum_builder.rx, second_client_order_id).await;

    let commission: Decimal = order_snapshot
        .fills
        .fills
        .iter()
        .map(|fill| fill.commission_amount())
        .sum();

    assert_eq!(first_amount, order_snapshot.filled_amount());
    assert_eq!(dec!(0.004), commission);
}

#[ignore = "need solana keypair"]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn full_order_fill() {
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
    let second_amount = dec!(9);
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
    let first_builder_exchange = first_serum_builder.exchange.clone();
    defer! {
        tokio::spawn(async move {
            first_order_proxy
                .cancel_order_or_fail(&first_order_ref, first_builder_exchange.clone())
                .await;
        });
    }

    let second_order_ref = second_order_proxy
        .create_order(second_serum_builder.exchange.clone())
        .await
        .expect("Create second order failed with error");
    let second_client_order_id = second_order_ref.client_order_id();
    let second_builder_exchange = second_serum_builder.exchange.clone();
    defer! {
        tokio::spawn(async move {
            second_order_proxy
                .cancel_order_or_fail(&second_order_ref, second_builder_exchange.clone())
                .await;
        });
    }

    let order_snapshot =
        receive_exchange_order_event(&mut second_serum_builder.rx, second_client_order_id).await;

    assert_eq!(second_amount, order_snapshot.filled_amount());
}

async fn receive_exchange_order_event(
    receiver: &mut broadcast::Receiver<ExchangeEvent>,
    client_order_id: ClientOrderId,
) -> Arc<OrderSnapshot> {
    // we can get another event first
    for _attempt in 0..10 {
        let event = receiver.recv().await.expect("Failed to get exchange event");
        if let ExchangeEvent::OrderEvent(order_event) = &event {
            if let OrderEventType::OrderFilled {
                cloned_order: order,
            } = &order_event.event_type
            {
                if client_order_id == order.header.client_order_id {
                    return order.clone();
                }
            }
        }
    }

    panic!("Should receive OrderEvent")
}
