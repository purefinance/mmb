#![cfg(test)]
use crate::binance::common::get_min_amount;
use crate::binance::common::{default_currency_pair, get_prices};
use crate::get_binance_credentials_or_exit;
use binance::binance::Binance;
use binance::binance::BinanceBuilder;
use core_tests::order::OrderProxy;
use jsonrpc_core::Value;
use jsonrpc_core_client::transports::ipc;
use mmb_core::config::parse_settings;
use mmb_core::exchanges::general::exchange::get_specific_currency_pair_for_tests;
use mmb_core::infrastructure::spawn_future_ok;
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::settings::CurrencyPairSetting;
use mmb_domain::market::CurrencyPair;
use mmb_rpc::rest_api::{MmbRpcClient, IPC_ADDRESS};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::{SpawnFutureFlags, WithExpect};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orders_cancelled() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let config = EngineBuildConfig::new(vec![Box::new(BinanceBuilder)]);

    let credentials =
        format!("[Binance_0]\napi_key = \"{api_key}\"\nsecret_key = \"{secret_key}\"");

    let settings = include_str!("control_panel.toml");
    let mut settings =
        parse_settings::<TestStrategySettings>(settings, &credentials).expect("in test");

    settings.core.exchanges[0].api_key = api_key.clone();
    settings.core.exchanges[0].api_key = secret_key;

    let init_settings = InitSettings::Directly(settings.clone());
    let engine = launch_trading_engine(&config, init_settings)
        .await
        .expect("in test");

    let exchange_settings = settings.core.exchanges[0].clone();
    let context = engine.context().clone();
    let exchange = context
        .exchanges
        .get(&exchange_settings.exchange_account_id)
        .expect("in test");

    if let CurrencyPairSetting::Ordinary { base, quote } = settings
        .core
        .exchanges
        .first()
        .and_then(|exchange_settings| exchange_settings.currency_pairs.as_ref())
        .and_then(|x| x.first())
        .expect("in test")
    {
        let test_currency_pair = CurrencyPair::from_codes(*base, *quote);
        let symbol = exchange
            .symbols
            .get(&test_currency_pair)
            .with_expect(|| format!("Can't find symbol {test_currency_pair})"))
            .value()
            .clone();
        let _ = exchange.cancel_all_orders(test_currency_pair).await;
        let (execution_price, min_price) = get_prices(
            get_specific_currency_pair_for_tests(&exchange, test_currency_pair),
            &Binance::make_hosts(exchange_settings.is_margin_trading),
            &exchange_settings,
            &symbol.price_precision,
        )
        .await;

        let amount = get_min_amount(
            get_specific_currency_pair_for_tests(&exchange, test_currency_pair),
            &Binance::make_hosts(exchange_settings.is_margin_trading),
            &exchange_settings,
            execution_price,
            &symbol,
        )
        .await;

        let order = OrderProxy::new(
            exchange_settings.exchange_account_id,
            Some("FromOrdersCancelledTest".to_owned()),
            CancellationToken::default(),
            min_price,
            amount,
            default_currency_pair(),
        );
        let created_order = order.create_order(exchange.clone()).await.expect("in test");
        order
            .cancel_order_or_fail(&created_order, exchange.clone())
            .await;
    } else {
        panic!("Can't read currency pair from settings");
    }

    let rest_client = ipc::connect::<_, MmbRpcClient>(IPC_ADDRESS)
        .await
        .expect("Failed to connect to the IPC socket");

    let statistics = Value::from_str(
        rest_client
            .stats()
            .await
            .expect("failed to get stats")
            .as_str(),
    )
    .expect("failed to conver answer to Value");

    let exchange_statistics = &statistics["market_account_id_stats"]["Binance_0|btc/usdt"];
    let opened_orders_count = exchange_statistics["opened_orders_count"]
        .as_u64()
        .expect("in test");
    let canceled_orders_count = exchange_statistics["canceled_orders_count"]
        .as_u64()
        .expect("in test");

    // Only one order was created and cancelled
    assert_eq!(opened_orders_count, 1);
    assert_eq!(canceled_orders_count, 1);

    let context = context.clone();
    let action = async move {
        sleep(Duration::from_millis(200)).await;
        context
            .clone()
            .lifetime_manager
            .run_graceful_shutdown("test")
            .await;
    };
    spawn_future_ok(
        "run graceful_shutdown in launch_engine test",
        SpawnFutureFlags::DENY_CANCELLATION | SpawnFutureFlags::STOP_BY_TOKEN,
        action,
    );

    engine.run().await;
}
