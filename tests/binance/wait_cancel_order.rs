use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;

use crate::core::exchange::ExchangeBuilder;
use crate::core::order::Order;

#[actix_rt::test]
async fn cancellation_waited_successfully() {
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

    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromCancellationWaitedSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );

    let created_order = order.create(exchange_builder.exchange.clone()).await;

    match created_order {
        Ok(order_ref) => {
            // If here are no error - order was cancelled successfully
            exchange_builder
                .exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await
                .expect("in test");
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn cancellation_waited_failed_fallback() {
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
            AllowedEventSourceType::FallbackOnly,
        ),
        Commission::default(),
        true,
    )
    .await
    .expect("in test");

    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromCancellationWaitedFailedFallbackTest".to_owned()),
        CancellationToken::default(),
    );

    let created_order = order.create(exchange_builder.exchange.clone()).await;

    match created_order {
        Ok(order_ref) => {
            let must_be_error = exchange_builder
                .exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await;
            match must_be_error {
                Ok(_) => assert!(false),
                Err(error) => {
                    assert_eq!(
                        "Order was expected to cancel explicity via Rest or Web Socket but got timeout instead",
                        &error.to_string()[..85]
                    );
                }
            }
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}
