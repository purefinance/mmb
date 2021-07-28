use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::events::ExchangeEvent;
use mmb_lib::core::exchanges::general::exchange::*;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::exchanges::{binance::binance::*, general::commission::Commission};
use mmb_lib::core::exchanges::{common::*, timeouts::timeout_manager::TimeoutManager};
use mmb_lib::core::lifecycle::application_manager::ApplicationManager;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::logger::init_logger;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::orders::pool::OrderRef;
use mmb_lib::core::settings;
use rust_decimal_macros::*;
use tokio::sync::broadcast;
use tokio::time::Duration;

use super::common::get_timeout_manager;
use crate::get_binance_credentials_or_exit;
use mmb_lib::core::exchanges::traits::ExchangeClientBuilder;

fn prepare_exchange(
    exchange_account_id: &ExchangeAccountId,
    api_key: String,
    secret_key: String,
    tx: broadcast::Sender<ExchangeEvent>,
    timeout_manager: Arc<TimeoutManager>,
) -> Arc<Exchange> {
    let mut settings = settings::ExchangeSettings::new_short(
        exchange_account_id.clone(),
        api_key,
        secret_key,
        false,
    );
    let application_manager = ApplicationManager::new(CancellationToken::default());

    BinanceBuilder.extend_settings(&mut settings);
    settings.websocket_channels = vec!["depth".into(), "trade".into()];
    let binance = Box::new(Binance::new(
        exchange_account_id.clone(),
        settings.clone(),
        tx.clone(),
        application_manager.clone(),
    ));

    Exchange::new(
        exchange_account_id.clone(),
        binance,
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
    )
}

async fn create_order(
    exchange_account_id: &ExchangeAccountId,
    exchange: &Arc<Exchange>,
    strategy_name: String,
) -> Result<OrderRef> {
    create_order_by_uid(
        &exchange_account_id,
        exchange,
        &ClientOrderId::unique_id(),
        strategy_name,
    )
    .await
}

async fn create_order_by_uid(
    exchange_account_id: &ExchangeAccountId,
    exchange: &Arc<Exchange>,
    id: &ClientOrderId,
    strategy_name: String,
) -> Result<OrderRef> {
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let test_price = dec!(0.0000001);
    let order_header = OrderHeader::new(
        id.clone(),
        Utc::now(),
        exchange_account_id.clone(),
        test_currency_pair.clone(),
        OrderType::Limit,
        OrderSide::Buy,
        dec!(10000),
        OrderExecutionType::None,
        None,
        None,
        strategy_name.to_owned(),
    );
    create_order_by_header(&exchange, &order_header, test_currency_pair, test_price).await
}

async fn create_order_by_header(
    exchange: &Arc<Exchange>,
    order_header: &Arc<OrderHeader>,
    test_currency_pair: CurrencyPair,
    test_price: rust_decimal::Decimal,
) -> Result<OrderRef> {
    let order_to_create = OrderCreating {
        header: order_header.clone(),
        price: test_price,
    };

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

    match created_order {
        Ok(order_ref) => Ok(order_ref),
        Err(error) => {
            dbg!(&error);
            assert!(false);
            Err(error)
        }
    }
}

#[actix_rt::test]
async fn cancelled_successfully() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let (tx, _rx) = broadcast::channel(10);

    let exchange = prepare_exchange(
        &exchange_account_id,
        api_key,
        secret_key,
        tx,
        TimeoutManager::new(HashMap::new()),
    );
    exchange.clone().connect().await;

    let test_order_client_id = ClientOrderId::unique_id();
    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let test_price = dec!(0.0000001);
    let order_header = OrderHeader::new(
        test_order_client_id,
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

    match create_order_by_header(&exchange, &order_header, test_currency_pair, test_price).await {
        Ok(order_ref) => {
            let exchange_order_id = order_ref.exchange_order_id().expect("in test");
            let order_to_cancel = OrderCancelling {
                header: order_header,
                exchange_order_id,
            };

            // Cancel last order
            let cancel_outcome = exchange
                .cancel_order(&order_to_cancel, CancellationToken::default())
                .await
                .expect("in test")
                .expect("in test");

            if let RequestResult::Success(gotten_client_order_id) = cancel_outcome.outcome {
                assert_eq!(
                    gotten_client_order_id,
                    order_to_cancel.header.client_order_id
                );
            }
        }

        // Create order failed
        Err(error) => {
            dbg!(&error);
            assert!(false)
        }
    }
}

#[actix_rt::test]
async fn cancel_opened_orders_successfully() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let (tx, _rx) = broadcast::channel(10);

    let exchange = prepare_exchange(
        &exchange_account_id,
        api_key,
        secret_key,
        tx,
        get_timeout_manager(&exchange_account_id),
    );
    exchange.clone().connect().await;

    let _ = create_order(
        &exchange_account_id,
        &exchange,
        "FromCancelOrderTest".to_string(),
    )
    .await;
    let _ = create_order(
        &exchange_account_id,
        &exchange,
        "FromCancelOrderTest".to_string(),
    )
    .await;

    process_open_orders(false, &exchange).await;
    process_open_orders(true, &exchange).await;
}

async fn process_open_orders(is_expecting_empty: bool, exchange: &Arc<Exchange>) {
    match &exchange.get_open_orders().await {
        Err(error) => {
            log::log!(
                log::Level::Info,
                "Opened orders not found for exchange account id: {}",
                error,
            );
            if !is_expecting_empty {
                assert!(false);
            }
        }
        Ok(_orders) => {
            if is_expecting_empty {
                assert!(false);
            }
            &exchange.clone().cancel_opened_orders().await;
        }
    }
}

#[actix_rt::test]
async fn nothing_to_cancel() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    init_logger();

    let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

    let (tx, _rx) = broadcast::channel(10);

    let exchange = prepare_exchange(
        &exchange_account_id,
        api_key,
        secret_key,
        tx,
        TimeoutManager::new(HashMap::new()),
    );
    exchange.clone().connect().await;

    let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
    let generated_client_order_id = ClientOrderId::unique_id();
    let order_header = OrderHeader::new(
        generated_client_order_id.clone(),
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

    let order_to_cancel = OrderCancelling {
        header: order_header,
        exchange_order_id: "1234567890".into(),
    };

    // Should be called before any other api calls!
    exchange.build_metadata().await;
    // Cancel last order
    let cancel_outcome = exchange
        .cancel_order(&order_to_cancel, CancellationToken::default())
        .await
        .expect("in test")
        .expect("in test");

    if let RequestResult::Error(error) = cancel_outcome.outcome {
        assert_eq!(error.error_type, ExchangeErrorType::OrderNotFound);
    }
}
