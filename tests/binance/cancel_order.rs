use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;

use crate::core::exchange::ExchangeTest;
use crate::core::order::Order;

#[actix_rt::test]
async fn cancelled_successfully() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange = ExchangeTest::try_new(
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
    )
    .await;
    if let Err(error) = exchange {
        log::warn!("{:?}", error);
        return;
    }

    let exchange = exchange.unwrap();

    let order = Order::new(
        None,
        exchange_account_id.clone(),
        Some("FromCancelOrderTest".to_string()),
        CancellationToken::default(),
    );

    match order.create(exchange.exchange.clone()).await {
        Ok(order_ref) => {
            order.cancel(&order_ref, exchange.exchange).await;
        }
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn nothing_to_cancel() {
    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let exchange = ExchangeTest::try_new(
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
    )
    .await;

    if let Err(error) = exchange {
        log::warn!("{:?}", error);
        return;
    }

    let exchange = exchange.unwrap();

    let order = Order::new(
        None,
        exchange_account_id.clone(),
        Some("FromCancelOrderTest".to_string()),
        CancellationToken::default(),
    );

    let order_to_cancel = OrderCancelling {
        header: order.header,
        exchange_order_id: "1234567890".into(),
    };

    // Cancel last order
    let cancel_outcome = exchange
        .exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test")
        .expect("in test");

    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
