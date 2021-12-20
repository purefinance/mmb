pub use std::collections::HashMap;

use mmb_core::core::exchanges::common::*;
use mmb_core::core::exchanges::events::AllowedEventSourceType;
use mmb_core::core::exchanges::general::commission::Commission;
use mmb_core::core::exchanges::general::features::*;
use mmb_core::core::orders::order::*;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger;

use crate::binance::binance_builder::BinanceBuilder;
use core_tests::order::OrderProxy;

#[actix_rt::test]
async fn get_order_info() {
    init_logger();
    let exchange_account_id: ExchangeAccountId = "Binance_0".parse().expect("in test");
    let binance_builder = match BinanceBuilder::try_new(
        exchange_account_id,
        CancellationToken::default(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
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

    let mut order_proxy = OrderProxy::new(
        exchange_account_id,
        Some("FromGetOrderInfoTest".to_owned()),
        CancellationToken::default(),
        binance_builder.default_price,
    );
    order_proxy.reservation_id = Some(ReservationId::generate());

    let created_order = order_proxy
        .create_order(binance_builder.exchange.clone())
        .await
        .expect("in test");

    let order_info = binance_builder
        .exchange
        .get_order_info(&created_order)
        .await
        .expect("in test");

    let created_exchange_order_id = created_order.exchange_order_id().expect("in test");
    let gotten_info_exchange_order_id = order_info.exchange_order_id;

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
