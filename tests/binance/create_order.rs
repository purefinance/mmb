use mmb::exchanges::{events::AllowedEventSourceType, general::commission::Commission};
use mmb_lib::core as mmb;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::event::OrderEventType;
use rust_decimal_macros::*;

use mmb_lib::core::exchanges::events::ExchangeEvent;

use crate::core::exchange::ExchangeBuilder;
use crate::core::order::Order;
#[actix_rt::test]
async fn create_successfully() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let mut exchange_builder = ExchangeBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    .expect("in test");

    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromCreateSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );

    match order.create(exchange_builder.exchange.clone()).await {
        Ok(order_ref) => {
            let event = exchange_builder
                .rx
                .recv()
                .await
                .expect("CreateOrderSucceeded event had to be occurred");

            let order_event = if let ExchangeEvent::OrderEvent(order_event) = event {
                order_event
            } else {
                panic!("Should receive OrderEvent")
            };

            if let OrderEventType::CreateOrderSucceeded = order_event.event_type {
            } else {
                panic!("Should receive CreateOrderSucceeded event type")
            }

            order
                .cancel(&order_ref, exchange_builder.exchange.clone())
                .await;
        }

        // Create order failed
        Err(_) => {
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn should_fail() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange_builder = ExchangeBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await
    .expect("in test");

    let mut order = Order::new(
        exchange_account_id.clone(),
        Some("FromShouldFailTest".to_owned()),
        CancellationToken::default(),
    );
    order.amount = dec!(1);
    order.price = dec!(0.0000000000000000001);

    match order.create(exchange_builder.exchange.clone()).await {
        Ok(_) => {
            assert!(false)
        }
        Err(error) => {
            assert_eq!(
                "Exchange error: Precision is over the maximum defined for this asset.",
                error.to_string()
            );
        }
    }
}
