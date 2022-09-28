use crate::bitmex::bitmex_builder::{BitmexBuilder, EXCHANGE_ACCOUNT_ID};
use crate::bitmex::common::get_bitmex_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_domain::market::{CurrencyPair, ExchangeAccountId};
use mmb_domain::order::snapshot::OrderSide;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use rust_decimal_macros::dec;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_open_orders_simple() {
    init_logger_file_named("log.txt");

    let (api_key, secret_key) = match get_bitmex_credentials() {
        Ok((api_key, secret_key)) => (api_key, secret_key),
        Err(_) => return,
    };
    let exchange_account_id: ExchangeAccountId = EXCHANGE_ACCOUNT_ID.parse().expect("in test");
    let mut settings = ExchangeSettings::new_short(exchange_account_id, api_key, secret_key, false);
    settings.currency_pairs = Some(vec![CurrencyPairSetting::Ordinary {
        base: "XBT".into(),
        quote: "USD".into(),
    }]);
    let features_0 = ExchangeFeatures::new(
        OpenOrdersType::AllCurrencyPair,
        RestFillsFeatures::default(),
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
    let bitmex_builder_0 =
        BitmexBuilder::build_account_with_setting(settings.clone(), features_0, false).await;

    let mut order_proxy_0 = OrderProxy::new(
        bitmex_builder_0.exchange.exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        CancellationToken::default(),
        dec!(10000),
        dec!(100),
    );
    order_proxy_0.timeout = Duration::from_secs(15);
    order_proxy_0.currency_pair = CurrencyPair::from_codes("xbt".into(), "usd".into());
    order_proxy_0.side = OrderSide::Buy;

    order_proxy_0
        .create_order(bitmex_builder_0.exchange.clone())
        .await
        .expect("Create order failed with error");

    let all_orders = bitmex_builder_0
        .exchange
        .get_open_orders(false)
        .await
        .expect("Failed to get open orders");

    let features_1 = ExchangeFeatures::new(
        OpenOrdersType::OneCurrencyPair,
        RestFillsFeatures::default(),
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

    let bitmex_builder_1 =
        BitmexBuilder::build_account_with_setting(settings, features_1, false).await;

    let mut order_proxy_1 = OrderProxy::new(
        bitmex_builder_1.exchange.exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        CancellationToken::default(),
        dec!(10000),
        dec!(100),
    );
    order_proxy_1.timeout = Duration::from_secs(15);
    order_proxy_1.currency_pair = CurrencyPair::from_codes("xbt".into(), "usd".into());
    order_proxy_1.side = OrderSide::Buy;

    order_proxy_1
        .create_order(bitmex_builder_1.exchange.clone())
        .await
        .expect("Create order failed with error");

    let currency_pair_orders = bitmex_builder_1
        .exchange
        .get_open_orders(false)
        .await
        .expect("Failed to get open orders");

    bitmex_builder_0
        .exchange
        .cancel_all_orders(order_proxy_0.currency_pair)
        .await
        .expect("Failed to cancel all orders");
    bitmex_builder_1
        .exchange
        .cancel_all_orders(order_proxy_1.currency_pair)
        .await
        .expect("Failed to cancel all orders");

    assert_eq!(all_orders.len(), 1);
    assert_eq!(currency_pair_orders.len(), 1);
}
