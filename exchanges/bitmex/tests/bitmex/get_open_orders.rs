use crate::bitmex::bitmex_builder::{BitmexBuilder, EXCHANGE_ACCOUNT_ID};
use crate::bitmex::common::get_bitmex_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_domain::market::ExchangeAccountId;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::logger::init_logger_file_named;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_orders_exists() {
    init_logger_file_named("log.txt");

    let bitmex_builder = match BitmexBuilder::build_account(false).await {
        Ok(bitmex_builder) => bitmex_builder,
        Err(_) => return,
    };
    let exchange_account_id = bitmex_builder.exchange.exchange_account_id;

    let order_proxy1 = OrderProxy::new(
        exchange_account_id,
        Some("FromOpenOrdersExistsTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.default_price,
        bitmex_builder.min_amount,
    );

    let order_proxy2 = OrderProxy::new(
        exchange_account_id,
        Some("FromOpenOrdersExistsTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.default_price,
        bitmex_builder.min_amount,
    );

    let _ = order_proxy1
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order1 failed with");

    let _ = order_proxy2
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("Create order2 failed");

    let all_orders = bitmex_builder
        .exchange
        .clone()
        .get_open_orders(false)
        .await
        .expect("in test");

    bitmex_builder
        .exchange
        .cancel_opened_orders(CancellationToken::default(), true)
        .await;

    assert_eq!(all_orders.len(), 2);
}

// TODO Delete after cancel_order() implementation
// Test is only to check open orders which are created manually with web interface (one XBTUSD order by default)
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

    let all_orders = bitmex_builder_0
        .exchange
        .get_open_orders(false)
        .await
        .expect("Failed to get open orders");

    println!("All open orders:\n{all_orders:?}");

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

    let currency_pair_orders = bitmex_builder_1
        .exchange
        .get_open_orders(false)
        .await
        .expect("Failed to get open orders");

    println!("Open orders by currency pair:\n{currency_pair_orders:?}");

    // assert_eq!(all_orders.len(), 1);
    assert_eq!(currency_pair_orders.len(), 1);
}
