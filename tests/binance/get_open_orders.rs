use std::time::Duration;

use chrono::Utc;
use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::exchange_creation;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, events::AllowedEventSourceType};
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::settings;
use mmb_lib::core::settings::CurrencyPairSetting;
use rust_decimal_macros::*;
use smallstr::SmallString;

use super::common::get_timeout_manager;
use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use tokio::sync::broadcast;

#[actix_rt::test]
async fn open_orders_exist() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, _rx) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];

    let binance = Binance::new(
        exchange_account_id.clone(),
        settings,
        tx.clone(),
        application_manager.clone(),
    );

    let timeout_manager = get_timeout_manager(&exchange_account_id);
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
        timeout_manager,
        Commission::default(),
    );

    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let test_price = dec!(0.00000005);
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

    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    const TIMEOUT: Duration = Duration::from_secs(5);
    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    if let Err(error) = created_order {
        dbg!(&error);
        assert!(false)
    }

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
    let _ = exchange.get_open_orders(true).await.expect("in test");

    let created_order_fut =
        exchange.create_order(&second_order_to_create, CancellationToken::default());

    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    match created_order {
        Ok(_order_ref) => {
            let all_orders = exchange.get_open_orders(true).await.expect("in test");
            assert!(!all_orders.is_empty())
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn open_orders_by_currency_pair_exist() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, _rx) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    settings.currency_pairs = Some(vec![
        CurrencyPairSetting {
            base: CurrencyCode::new(SmallString::from("phb")),
            quote: CurrencyCode::new(SmallString::from("btc")),
            currency_pair: None,
        },
        CurrencyPairSetting {
            base: CurrencyCode::new(SmallString::from("troy")),
            quote: CurrencyCode::new(SmallString::from("btc")),
            currency_pair: None,
        },
    ]);
    let binance = Binance::new(
        exchange_account_id.clone(),
        settings.clone(),
        tx.clone(),
        application_manager.clone(),
    );
    let timeout_manager = get_timeout_manager(&exchange_account_id);
    let exchange = Exchange::new(
        exchange_account_id.clone(),
        Box::new(binance),
        ExchangeFeatures::new(
            OpenOrdersType::OneCurrencyPair,
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        tx,
        application_manager,
        timeout_manager,
        Commission::default(),
    );

    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let second_test_currency_pair = CurrencyPair::from_codes("troy".into(), "btc".into());

    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(2000),
        OrderExecutionType::None,
        None,
        None,
        "FromGetOpenOrdersTest".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header,
        price: dec!(0.0000001),
    };

    // Should be called before any other api calls!
    exchange.build_metadata().await;
    if let Some(currency_pairs) = &settings.currency_pairs {
        exchange.set_symbols(exchange_creation::get_symbols(
            &exchange,
            &currency_pairs[..],
        ))
    }
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");

    let _ = exchange
        .cancel_all_orders(second_test_currency_pair.clone())
        .await
        .expect("in test");

    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    const TIMEOUT: Duration = Duration::from_secs(5);
    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    if let Err(error) = created_order {
        dbg!(&error);
        assert!(false)
    }

    let second_test_order_client_id = ClientOrderId::unique_id();
    let second_order_header = OrderHeader::new(
        second_test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        second_test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(2000),
        OrderExecutionType::None,
        None,
        None,
        "FromGetOpenOrdersTest".to_owned(),
    );
    let second_order_to_create = OrderCreating {
        header: second_order_header,
        price: dec!(0.0000001),
    };

    let created_order_fut =
        exchange.create_order(&second_order_to_create, CancellationToken::default());

    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    if let Err(error) = created_order {
        dbg!(&error);
        assert!(false)
    }

    let all_orders = exchange.get_open_orders(true).await.expect("in test");

    // TODO: change to cancel_opened_orders
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");

    let _ = exchange
        .cancel_all_orders(second_test_currency_pair.clone())
        .await
        .expect("in test");

    assert_eq!(all_orders.len(), 2);

    for order in all_orders {
        assert!(
            order.client_order_id == test_order_client_id
                || order.client_order_id == second_test_order_client_id
        );
    }
}
#[actix_rt::test]
async fn should_return_open_orders() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );

    let application_manager = ApplicationManager::new(CancellationToken::new());
    let (tx, _rx) = broadcast::channel(10);

    BinanceBuilder.extend_settings(&mut settings);
    let binance = Binance::new(
        exchange_account_id.clone(),
        settings.clone(),
        tx.clone(),
        application_manager.clone(),
    );
    let timeout_manager = get_timeout_manager(&exchange_account_id);
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
        timeout_manager,
        Commission::default(),
    );

    exchange.clone().connect().await;

    // Should be called before any other api calls!
    exchange.build_metadata().await;

    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    const TIMEOUT: Duration = Duration::from_secs(5);

    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");

    // createdOrder
    let test_order_client_id = ClientOrderId::unique_id();
    let order_header = OrderHeader::new(
        test_order_client_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(2000),
        OrderExecutionType::None,
        None,
        None,
        "ShouldReturnOpenOrders".to_owned(),
    );

    let order_to_create = OrderCreating {
        header: order_header,
        price: dec!(0.0000001),
    };

    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    if let Err(error) = created_order {
        dbg!(&error);
        assert!(false)
    }
    // createdOrder

    // orderForCancellation
    let order_for_cancellation_id = ClientOrderId::unique_id();

    let order_header = OrderHeader::new(
        order_for_cancellation_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(2000),
        OrderExecutionType::None,
        None,
        None,
        "ShouldReturnOpenOrders".to_owned(),
    );
    let order_to_create = OrderCreating {
        header: order_header,
        price: dec!(0.0000001),
    };

    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    match created_order {
        Ok(order_ref) => {
            // If here are no error - order was cancelled successfully
            exchange
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

    // orderForCancellation

    // failedToCreateOrder
    let order_for_cancelation_id = ClientOrderId::unique_id();

    let order_header = OrderHeader::new(
        order_for_cancelation_id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(0), // zero amount
        OrderExecutionType::None,
        None,
        None,
        "ShouldReturnOpenOrders".to_owned(),
    );
    let order_to_create = OrderCreating {
        header: order_header,
        price: dec!(0.0000001),
    };

    let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());

    let created_order = tokio::select! {
        created_order = created_order_fut => created_order,
        _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
    };

    if let Ok(order_ref) = created_order {
        dbg!(&order_ref);
        assert!(false)
    }
    // failedToCreateOrder

    // TODO: orderForCompletion

    let all_orders = exchange.get_open_orders(true).await.expect("in test");

    // TODO: change to cancel_opened_orders
    let _ = exchange
        .cancel_all_orders(test_currency_pair.clone())
        .await
        .expect("in test");

    assert_eq!(all_orders.len(), 1);
}
