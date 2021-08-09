use std::thread;
use std::time::Duration;

use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;

use crate::core::exchange::ExchangeBuilder;
use crate::core::order::Order;

#[actix_rt::test]
#[ignore]
async fn open_orders_exists() {
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

    let first_order = Order::new(
        exchange_account_id.clone(),
        Some("FromGetOpenOrdersTest".to_string()),
        CancellationToken::default(),
    );

    let second_order = Order::new(
        exchange_account_id.clone(),
        Some("FromGetOpenOrdersTest".to_string()),
        CancellationToken::default(),
    );

    if let Err(error) = first_order.create(exchange_builder.exchange.clone()).await {
        log::error!("{:?}", error);
        return;
    }

    if let Err(error) = second_order.create(exchange_builder.exchange.clone()).await {
        log::error!("{:?}", error);
        return;
    }
    // Binance can process new orders close to 10 seconds
    thread::sleep(Duration::from_secs(10));

    let all_orders = exchange_builder
        .exchange
        .clone()
        .get_open_orders(false)
        .await
        .expect("in test");

    assert_eq!(all_orders.len(), 1);
}
