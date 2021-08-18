use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;

use crate::binance::binance_builder::BinanceBuilder;
use crate::core::order::OrderProxy;

#[actix_rt::test]
async fn cancellation_waited_successfully() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
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
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromCancellationWaitedSuccessfullyTest".to_owned()),
        CancellationToken::default(),
    );

    let created_order = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await;

    match created_order {
        Ok(order_ref) => {
            // If here are no error - order was cancelled successfully
            binance_builder
                .exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await
                .expect("in test");
        }

        Err(error) => {
            assert!(false, "Create order failed with error {:?}.", error)
        }
    }
}

#[actix_rt::test]
async fn cancellation_waited_failed_fallback() {
    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
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
    {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let order_proxy = OrderProxy::new(
        exchange_account_id.clone(),
        Some("FromCancellationWaitedFailedFallbackTest".to_owned()),
        CancellationToken::default(),
    );

    let created_order = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await;

    match created_order {
        Ok(order_ref) => {
            let must_be_error = binance_builder
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

        Err(error) => {
            assert!(false, "Create order failed with error {:?}.", error)
        }
    }
}
