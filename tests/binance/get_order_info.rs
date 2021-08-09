pub use std::collections::HashMap;

use mmb_lib::core::exchanges::common::*;
use mmb_lib::core::exchanges::events::AllowedEventSourceType;
use mmb_lib::core::exchanges::general::commission::Commission;
use mmb_lib::core::exchanges::general::features::*;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::orders::order::*;

use crate::core::exchange::ExchangeBuilder;
use crate::core::order::Order;

#[actix_rt::test]
async fn get_order_info() {
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

    let mut order = Order::new(
        exchange_account_id.clone(),
        Some("FromGetOrderInfoTest".to_string()),
        CancellationToken::default(),
    );
    order.reservation_id = Some(ReservationId::generate());

    let created_order = order
        .create(exchange_builder.exchange.clone())
        .await
        .expect("in test");

    let order_info = exchange_builder
        .exchange
        .get_order_info(&created_order)
        .await
        .expect("in test");

    let created_exchange_order_id = created_order.exchange_order_id().expect("in test");
    let gotten_info_exchange_order_id = order_info.exchange_order_id;

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
