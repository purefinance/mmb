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
        Some("FromCancelOrderTest".to_string()),
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

    match &exchange.get_open_orders(false).await {
        Err(error) => {
            log::info!("Opened orders not found for exchange account id: {}", error,);
            assert!(false);
        }
        Ok(orders) => {
            assert_ne!(orders.len(), 0);
            &exchange
                .clone()
                .cancel_opened_orders(CancellationToken::default())
                .await;
        }
    }

    match &exchange.get_open_orders(false).await {
        Err(error) => {
            log::info!("Opened orders not found for exchange account id: {}", error,);
            assert!(false);
        }
        Ok(orders) => {
            assert_eq!(orders.len(), 0);
        }
    }
}

// #[actix_rt::test]
// async fn cancel_opened_orders_successfully() {
//     let (api_key, secret_key) = get_binance_credentials_or_exit!();

//     init_logger();

//     let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

//     let mut settings = settings::ExchangeSettings::new_short(
//         exchange_account_id.clone(),
//         api_key,
//         secret_key,
//         false,
//     );

//     let application_manager = ApplicationManager::new(CancellationToken::default());
//     let (tx, _rx) = broadcast::channel(10);

//     BinanceBuilder.extend_settings(&mut settings);
//     settings.websocket_channels = vec!["depth".into(), "trade".into()];

//     let binance = Box::new(Binance::new(
//         exchange_account_id.clone(),
//         settings.clone(),
//         tx.clone(),
//         application_manager.clone(),
//     ));

//     let timeout_manager = get_timeout_manager(&exchange_account_id);
//     let exchange = Exchange::new(
//         exchange_account_id.clone(),
//         binance,
//         ExchangeFeatures::new(
//             OpenOrdersType::AllCurrencyPair,
//             true,
//             true,
//             AllowedEventSourceType::default(),
//             AllowedEventSourceType::default(),
//         ),
//         tx,
//         application_manager,
//         timeout_manager,
//         Commission::default(),
//     );

//     exchange.clone().connect().await;

//     let test_order_client_id = ClientOrderId::unique_id();
//     let test_currency_pair = CurrencyPair::from_codes("phb".into(), "btc".into());
//     let test_price = dec!(0.0000001);
//     let order_header = OrderHeader::new(
//         test_order_client_id.clone(),
//         Utc::now(),
//         exchange_account_id.clone(),
//         test_currency_pair.clone(),
//         OrderType::Limit,
//         OrderSide::Buy,
//         dec!(2000),
//         OrderExecutionType::None,
//         None,
//         None,
//         "FromCancelOrderTest".to_owned(),
//     );

//     let order_to_create = OrderCreating {
//         header: order_header.clone(),
//         price: test_price,
//     };

//     // Should be called before any other api calls!
//     exchange.build_metadata().await;
//     let _ = exchange
//         .cancel_all_orders(test_currency_pair.clone())
//         .await
//         .expect("in test");
//     let created_order_fut = exchange.create_order(&order_to_create, CancellationToken::default());
//     const TIMEOUT: Duration = Duration::from_secs(5);
//     let created_order = tokio::select! {
//         created_order = created_order_fut => created_order,
//         _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
//     };

//     match created_order {
//         Ok(_order_ref) => {
//             let second_test_order_client_id = ClientOrderId::unique_id();
//             let second_order_header = OrderHeader::new(
//                 second_test_order_client_id.clone(),
//                 Utc::now(),
//                 exchange_account_id.clone(),
//                 test_currency_pair.clone(),
//                 OrderType::Limit,
//                 OrderSide::Buy,
//                 dec!(2000),
//                 OrderExecutionType::None,
//                 None,
//                 None,
//                 "FromCancelOrderTest".to_owned(),
//             );

//             let second_order_to_create = OrderCreating {
//                 header: second_order_header,
//                 price: test_price,
//             };

//             let created_order_fut =
//                 exchange.create_order(&second_order_to_create, CancellationToken::default());

//             let _ = tokio::select! {
//                 created_order = created_order_fut => created_order,
//                 _ = tokio::time::sleep(TIMEOUT) => panic!("Timeout {} secs is exceeded", TIMEOUT.as_secs())
//             }.expect("in test");
//         }

//         // Create order failed
//         Err(error) => {
//             dbg!(&error);
//             assert!(false)
//         }
//     }

//     match &exchange.get_open_orders(false).await {
//         Err(error) => {
//             log::info!("Opened orders not found for exchange account id: {}", error,);
//             assert!(false);
//         }
//         Ok(orders) => {
//             assert_ne!(orders.len(), 0);
//             &exchange
//                 .clone()
//                 .cancel_opened_orders(CancellationToken::default())
//                 .await;
//         }
//     }

//     match &exchange.get_open_orders(false).await {
//         Err(error) => {
//             log::info!("Opened orders not found for exchange account id: {}", error,);
//             assert!(false);
//         }
//         Ok(orders) => {
//             assert_eq!(orders.len(), 0);
//         }
//     }
// }

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
    .await
    .expect("in test");

    let order = Order::new(
        exchange_account_id.clone(),
        Some("FromNothingToCancelTest".to_string()),
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
