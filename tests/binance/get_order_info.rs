use crate::get_binance_credentials_or_exit;
use chrono::Utc;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, events::AllowedEventSourceType};
use mmb_lib::core::exchanges::{
    cancellation_token::CancellationToken, general::commission::Commission,
};
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use std::env;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::time::sleep;

#[actix_rt::test]
async fn get_order_info() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

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
        Some(ReservationId::gen_new()),
        None,
        "".into(),
    );

    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: dec!(0.0000001),
    };

    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    let created_order = exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await
        .expect("in test");

    // it seems that it does not have time to create the order without delay
    sleep(Duration::from_millis(500)).await;

    let order_info = exchange
        .get_order_info(&created_order)
        .await
        .expect("in test");

    let created_exchange_order_id = created_order.exchange_order_id().expect("in test");
    let gotten_info_exchange_order_id = order_info.exchange_order_id;

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
