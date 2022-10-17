use crate::bitmex::bitmex_builder::{default_exchange_account_id, BitmexBuilder};
use crate::bitmex::common::get_bitmex_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_open_orders() {
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
    let features = ExchangeFeatures::new(
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
    let bitmex_builder =
        BitmexBuilder::build_account_with_setting(settings.clone(), features).await;

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.min_price,
        bitmex_builder.min_amount,
        bitmex_builder.default_currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);

    let order_ref = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    let all_orders = bitmex_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Failed to get open orders");

    order_proxy
        .cancel_order_or_fail(&order_ref, bitmex_builder.exchange)
        .await;

    assert_eq!(all_orders.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_open_orders_by_currency_pair() {
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

    let features = ExchangeFeatures::new(
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

    let bitmex_builder = BitmexBuilder::build_account_with_setting(settings, features).await;

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromGetOpenOrdersTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.min_price,
        bitmex_builder.min_amount,
        bitmex_builder.default_currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);

    let order_ref = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order failed with error");

    let currency_pair_orders = bitmex_builder
        .exchange
        .get_open_orders(false)
        .await
        .expect("Failed to get open orders");

    order_proxy
        .cancel_order_or_fail(&order_ref, bitmex_builder.exchange)
        .await;

    assert_eq!(currency_pair_orders.len(), 1);
}
