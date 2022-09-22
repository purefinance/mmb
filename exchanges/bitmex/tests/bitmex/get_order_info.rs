use crate::bitmex::bitmex_builder::BitmexBuilder;
use core_tests::order::OrderProxy;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::snapshot::{OrderSide, ReservationId};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::dec;
use std::time::Duration;

// TODO Add order cancelling after cancel_order implementation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_order_info() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(false).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromGetOrderInfoTest".to_owned()),
        CancellationToken::default(),
        dec!(10000),
        dec!(100),
    );
    order_proxy.timeout = Duration::from_secs(15);
    order_proxy.currency_pair = CurrencyPair::from_codes("xbt".into(), "usd".into());
    order_proxy.side = OrderSide::Buy;
    order_proxy.reservation_id = Some(ReservationId::generate());

    let created_order = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("in test");

    let order_info = bitmex_builder
        .exchange
        .get_order_info(&created_order)
        .await
        .expect("in test");

    let created_exchange_order_id = created_order.exchange_order_id().expect("in test");
    let gotten_info_exchange_order_id = order_info.exchange_order_id;

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
