use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;

use crate::core::exchange::ExchangeBuilder;
use crate::core::order::Order;

#[actix_rt::test]
async fn cancelled_successfully() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange_builder = ExchangeBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await;

    if let Err(_) = exchange_builder {
        return;
    }
    let exchange_builder = exchange_builder.unwrap();

    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromCancelledSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );

    match order.create(exchange_builder.exchange.clone()).await {
        Ok(order_ref) => {
            order
                .cancel(&order_ref, exchange_builder.exchange.clone())
                .await;
        }
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn cancel_opened_orders_successfully() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange_builder = ExchangeBuilder::try_new(
        exchange_account_id.clone(),
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            true,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        Commission::default(),
        true,
    )
    .await;

    if let Err(_) = exchange_builder {
        return;
    }
    let exchange_builder = exchange_builder.unwrap();

    let first_order = Order::new(
        exchange_account_id.clone(),
        Some("FromCancelOpenedOrdersSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );
    first_order
        .create(exchange_builder.exchange.clone())
        .await
        .expect("in test");

    let second_order = Order::new(
        exchange_account_id.clone(),
        Some("FromCancelOpenedOrdersSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );
    second_order
        .create(exchange_builder.exchange.clone())
        .await
        .expect("in test");

    match &exchange_builder.exchange.get_open_orders(false).await {
        Err(error) => {
            log::info!("Opened orders not found for exchange account id: {}", error,);
            assert!(false);
        }
        Ok(orders) => {
            assert_ne!(orders.len(), 0);
            &exchange_builder
                .exchange
                .clone()
                .cancel_opened_orders(CancellationToken::default(), true)
                .await;
        }
    }

    match &exchange_builder.exchange.get_open_orders(false).await {
        Err(error) => {
            log::info!("Opened orders not found for exchange account id: {}", error,);
            assert!(false);
        }
        Ok(orders) => {
            assert_eq!(orders.len(), 0);
        }
    }
}

#[actix_rt::test]
async fn nothing_to_cancel() {
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
    .await;

    if let Err(_) = exchange_builder {
        return;
    }
    let exchange_builder = exchange_builder.unwrap();

    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromNothingToCancelTest".to_owned()),
        CancellationToken::default(),
    );
    let order_to_cancel = OrderCancelling {
        header: order.make_header(),
        exchange_order_id: "1234567890".into(),
    };

    // Cancel last order
    let cancel_outcome = exchange_builder
        .exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test")
        .expect("in test");
    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
