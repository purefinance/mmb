use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, events::AllowedEventSourceType};
use mmb_lib::core::exchanges::{common::*, timeouts::timeout_manager::TimeoutManager};
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use mmb_lib::core::{
    exchanges::general::commission::Commission, statistic_service::StatisticService,
};
use rust_decimal_macros::*;

use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use tokio::sync::broadcast;

#[actix_rt::test]
#[ignore]
async fn open_orders_exists() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, _) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];

    let binance = Binance::new(
        exchange_account_id.clone(),
        settings,
        tx.clone(),
        application_manager.clone(),
    );

    let exchange = Exchange::new(
        exchange_account_id.clone(),
        Box::new(binance),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        application_manager,
        TimeoutManager::new(HashMap::new()),
        Commission::default(),
    );

    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let test_price = dec!(0.00000007);
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
        "FromGetOpenOrdersTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header,
        price: test_price,
    };

    // Should be called before any other api calls!
    exchange.build_metadata().await;
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");
    exchange
        .create_order(&order_to_create, CancellationToken::default())
        .await
        .expect("in test");

    let second_test_order_client_id = ClientOrderId::unique_id();
    let second_order_header = OrderHeader::new(
        second_test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(10000),
        OrderExecutionType::None,
        None,
        None,
        "FromGetOpenOrdersTest".to_owned(),
    );

    let second_order_to_create = OrderCreating {
        header: second_order_header,
        price: test_price,
    };

    exchange
        .create_order(&second_order_to_create, CancellationToken::default())
        .await
        .expect("in test");

    // Binance can process new orders close to 10 seconds
    thread::sleep(Duration::from_secs(10));
    let all_orders = exchange.get_open_orders().await.expect("in test");

    assert!(!all_orders.is_empty())
}
