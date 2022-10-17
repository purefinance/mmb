use crate::bitmex::bitmex_builder::{default_exchange_account_id, BitmexBuilder};
use crate::bitmex::common::get_bitmex_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::exchanges::general::features::{
    ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption, RestFillsFeatures,
    RestFillsType, WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::logger::init_logger_file_named;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_my_trades() {
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

    let bitmex_builder = BitmexBuilder::build_account_with_setting(settings, features).await;

    let mut order_proxy = OrderProxy::new(
        bitmex_builder.exchange.exchange_account_id,
        Some("FromGetOrderInfoTest".to_owned()),
        CancellationToken::default(),
        bitmex_builder.execution_price,
        bitmex_builder.min_amount,
        bitmex_builder.default_currency_pair,
    );
    order_proxy.timeout = Duration::from_secs(15);

    let order_ref = order_proxy
        .create_order(bitmex_builder.exchange.clone())
        .await
        .expect("in test");

    let currency_pair = order_proxy.currency_pair;
    let symbol = bitmex_builder
        .exchange
        .symbols
        .get(&currency_pair)
        .with_expect(|| format!("Can't find symbol {currency_pair})"))
        .value()
        .clone();
    let trades = bitmex_builder
        .exchange
        .get_order_trades(&symbol, &order_ref)
        .await
        .expect("in test");

    match trades {
        RequestResult::Success(data) => {
            assert_eq!(data.len(), 1);
        }
        RequestResult::Error(err) => panic!("Failed to get trades: {err:?}"),
    }

    order_proxy
        .cancel_order_or_fail(&order_ref, bitmex_builder.exchange.clone())
        .await;
}
