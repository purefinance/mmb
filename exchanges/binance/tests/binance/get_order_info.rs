pub use std::collections::HashMap;

use mmb_domain::order::snapshot::*;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger;

use crate::binance::binance_builder::BinanceBuilder;
use core_tests::order::OrderProxy;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_order_info() {
    init_logger();

    let binance_builder = match BinanceBuilder::build_account_0().await {
        Ok(binance_builder) => binance_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        binance_builder.exchange.exchange_account_id,
        Some("FromGetOrderInfoTest".to_owned()),
        CancellationToken::default(),
        binance_builder.min_price,
        binance_builder.min_amount,
        binance_builder.default_currency_pair,
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

    order_proxy
        .cancel_order_or_fail(&created_order, binance_builder.exchange.clone())
        .await;

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
