use crate::get_binance_credentials_or_exit;
use chrono::Utc;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, general::commission::Commission};
use mmb_lib::core::exchanges::{
    cancellation_token::CancellationToken, events::AllowedEventSourceType,
};
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::sync::mpsc::channel;
use std::{env, sync::Arc};
use tokio::time::Duration;

#[actix_rt::test]
async fn cancellation_waited_successfully() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");
    let settings = settings::ExchangeSettings::new(
        exchange_account_id.clone(),
        api_key.expect("in test"),
        secret_key.expect("in test"),
        false,
    );

    let binance = Binance::new(settings, exchange_account_id.clone());

    let websocket_host = "wss://stream.binance.com:9443".into();
    let currency_pairs = vec!["PHBBTC".into()];
    let channels = vec!["depth".into(), "trade".into()];

    let (tx, _rx) = channel();
    let exchange = Exchange::new(
        exchange_account_id.clone(),
        websocket_host,
        currency_pairs,
        channels,
        Box::new(binance),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        Commission::default(),
    );

    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(10000),
        OrderExecutionType::None,
        None,
        None,
        "FromCancelOrderTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: dec!(0.0000001),
    };

    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    const TIMEOUT: Duration = Duration::from_secs(5);
    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    match created_order {
        Ok(order_ref) => {
            let exchange_order_id = order_ref.exchange_order_id().expect("in test");
            let order_to_cancel = OrderCancelling {
                header: Arc::new(order_header),
                exchange_order_id,
            };

            //// Cancel last order
            //let cancel_outcome = exchange
            //    .cancel_order(&order_to_cancel, CancellationToken::default())
            //    .await
            //    .expect("in test");

            let cancel_outcome = exchange
                .wait_cancel_order(order_ref, None, true, CancellationToken::new())
                .await;
            dbg!(&cancel_outcome);

            //if let RequestResult::Success(gotten_client_order_id) = cancel_outcome.outcome {
            //    assert_eq!(gotten_client_order_id, test_order_client_id);
            //}
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}