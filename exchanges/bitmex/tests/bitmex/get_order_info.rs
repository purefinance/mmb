use crate::bitmex::bitmex_builder::BitmexBuilder;
use core_tests::order::OrderProxy;
use mmb_domain::order::snapshot::ReservationId;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_order_info() {
    init_logger();

    let bitmex_builder = match BitmexBuilder::build_account(true).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromGetOrderInfoTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.min_price,
        bitmex_builder.min_amount,
        bitmex_builder.default_currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);
    order_proxy.reservation_id = Some(ReservationId::generate());

    let order_ref = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("in test");

    let order_info = bitmex_builder
        .exchange
        .get_order_info(&order_ref)
        .await
        .expect("in test");

    let created_exchange_order_id = order_ref.exchange_order_id().expect("in test");
    let gotten_info_exchange_order_id = order_info.exchange_order_id;

    order_proxy
        .cancel_order_or_fail(&order_ref, bitmex_builder.exchange.clone())
        .await;

    assert_eq!(created_exchange_order_id, gotten_info_exchange_order_id);
}
