use crate::bitmex::bitmex_builder::{default_exchange_account_id, BitmexBuilder};
use crate::bitmex::common::get_bitmex_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::features::{
    BalancePositionOption, ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption,
    RestFillsFeatures, RestFillsType, WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use std::thread;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_positions() {
    init_logger_file_named("log.txt");

    let (api_key, secret_key) = match get_bitmex_credentials() {
        Ok((api_key, secret_key)) => (api_key, secret_key),
        Err(_) => return,
    };
    let mut settings =
        ExchangeSettings::new_short(default_exchange_account_id(), api_key, secret_key, true);
    settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
        base: "XBT".into(),
        quote: "USD".into(),
    }]);

    let mut features = ExchangeFeatures::new(
        OpenOrdersType::OneCurrencyPair,
        RestFillsFeatures::new(RestFillsType::MyTrades),
        OrderFeatures {
            supports_get_order_info_by_client_order_id: true,
            ..OrderFeatures::default()
        },
        OrderTradeOption::default(),
        WebSocketOptions::default(),
        true,
        AllowedEventSourceType::default(),
        AllowedEventSourceType::default(),
        AllowedEventSourceType::default(),
    );
    features.balance_position_option = BalancePositionOption::IndividualRequests;

    let bitmex_builder = BitmexBuilder::build_account_with_setting(settings, features).await;

    let amount = bitmex_builder.min_amount;
    let currency_pair = bitmex_builder.default_currency_pair;
    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromCancelledSuccessfullyTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.execution_price,
        amount,
        currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);

    order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order failed with error:");

    // Need wait some time until order will be filled
    thread::sleep(Duration::from_secs(5));

    let active_positions = bitmex_builder
        .exchange
        .get_active_positions(order_proxy.cancellation_token.clone())
        .await;

    let position_info = active_positions.first().expect("Have no active positions");
    assert_eq!(
        (
            position_info.derivative.position,
            position_info.derivative.currency_pair,
            position_info.derivative.get_side()
        ),
        (amount.abs(), currency_pair, order_proxy.side)
    );

    let closed_position = bitmex_builder
        .exchange
        .close_position(position_info, None, order_proxy.cancellation_token.clone())
        .await
        .expect("Failed to get closed position");

    assert_eq!(closed_position.amount, amount);

    bitmex_builder
        .exchange
        .cancel_all_orders(order_proxy.currency_pair)
        .await
        .expect("Failed to cancel all orders");
}
