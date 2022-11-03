use crate::bitmex::bitmex_builder::{default_exchange_account_id, BitmexBuilder};
use crate::bitmex::common::get_bitmex_credentials;
use core_tests::order::OrderProxy;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::exchanges::general::features::{
    BalancePositionOption, ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption,
    RestFillsFeatures, RestFillsType, WebSocketOptions,
};
use mmb_core::settings::{CurrencyPairSetting, ExchangeSettings};
use mmb_domain::events::AllowedEventSourceType;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::logger::init_logger;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_my_trades() {
    init_logger();

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

    // Need wait some time until order will be filled
    let _ = sleep(Duration::from_secs(5));

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
            let trade = data.first().expect("No one trade received");
            assert_eq!(
                trade.exchange_order_id.clone(),
                order_ref
                    .exchange_order_id()
                    .expect("Failed to get order's exchange id"),
            )
        }
        RequestResult::Error(err) => panic!("Failed to get trades: {err:?}"),
    }

    let active_positions = bitmex_builder
        .exchange
        .get_active_positions(order_proxy.cancellation_token.clone())
        .await;
    let position_info = active_positions.first().expect("Have no active positions");

    let _ = bitmex_builder
        .exchange
        .close_position(position_info, None, order_proxy.cancellation_token.clone())
        .await
        .expect("Failed to get closed position");
}
