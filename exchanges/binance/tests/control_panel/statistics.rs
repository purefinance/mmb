#![cfg(test)]
use binance::binance::Binance;
use binance::binance::BinanceBuilder;
use domain::order::snapshot::OrderSnapshot;
use jsonrpc_core::Value;
use jsonrpc_core_client::transports::ipc;
use mmb_core::config::parse_settings;
use mmb_core::disposition_execution::{PriceSlot, TradingContext};
use mmb_core::explanation::Explanation;
use mmb_core::infrastructure::spawn_future_ok;
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use mmb_core::settings::BaseStrategySettings;
use mmb_core::settings::CurrencyPairSetting;
use mmb_core::strategies::disposition_strategy::DispositionStrategy;
use mmb_rpc::rest_api::{MmbRpcClient, IPC_ADDRESS};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::SpawnFutureFlags;
use mmb_utils::DateTime;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use crate::binance::common::get_default_price;
use crate::binance::common::get_min_amount;
use crate::get_binance_credentials_or_exit;
use core_tests::order::OrderProxy;
use domain::market::CurrencyPair;
use domain::market::ExchangeAccountId;
use domain::order::snapshot::Amount;
use mmb_core::exchanges::general::exchange::get_specific_currency_pair_for_tests;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

impl BaseStrategySettings for TestStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance_0".parse().expect("for testing")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("btc".into(), "usdt".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orders_cancelled() {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();
    struct TestStrategy;

    impl DispositionStrategy for TestStrategy {
        fn calculate_trading_context(
            &mut self,
            _now: DateTime,
            _local_snapshots_service: &LocalSnapshotsService,
            _explanation: &mut Explanation,
        ) -> Option<TradingContext> {
            None
        }

        fn handle_order_fill(
            &self,
            _cloned_order: &Arc<OrderSnapshot>,
            _price_slot: &PriceSlot,
            _target_eai: ExchangeAccountId,
            _cancellation_token: CancellationToken,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn configuration_descriptor(&self) -> ConfigurationDescriptor {
            ConfigurationDescriptor::new("TestStrategy".into(), "orders_cancelled_test".into())
        }
    }

    let config = EngineBuildConfig::new(vec![Box::new(BinanceBuilder)]);

    let credentials =
        format!("[Binance_0]\napi_key = \"{api_key}\"\nsecret_key = \"{secret_key}\"");

    let settings = include_str!("control_panel.toml");
    let mut settings =
        parse_settings::<TestStrategySettings>(settings, &credentials).expect("in test");

    let mut exchange_settings = &mut settings.core.exchanges[0];
    exchange_settings.api_key = api_key.clone();
    exchange_settings.secret_key = secret_key;
    let exchange_account_id = exchange_settings.exchange_account_id;

    let is_margin_trading = exchange_settings.is_margin_trading;
    let api_key = exchange_settings.api_key.clone();

    let init_settings = InitSettings::Directly(settings.clone());
    let engine = launch_trading_engine(&config, init_settings, |_, _| Box::new(TestStrategy))
        .await
        .expect("in test");

    let context = engine.context().clone();
    let exchange = context
        .exchanges
        .get(&exchange_account_id)
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
        let _ = exchange.cancel_all_orders(test_currency_pair).await;
        let price = get_default_price(
            get_specific_currency_pair_for_tests(&exchange, test_currency_pair),
            &Binance::make_hosts(is_margin_trading),
            &api_key,
            exchange_account_id,
            is_margin_trading,
        )
        .await;

        let symbol = exchange
            .symbols
            .get(&test_currency_pair)
            .expect("can't find symbol")
            .value()
            .clone();

        let amount = get_min_amount(
            get_specific_currency_pair_for_tests(&exchange, test_currency_pair),
            &Binance::make_hosts(is_margin_trading),
            &api_key,
            price,
            &symbol,
            exchange_account_id,
            is_margin_trading,
        )
        .await;

        let order = OrderProxy::new(
            exchange_account_id,
            Some("FromOrdersCancelledTest".to_owned()),
            CancellationToken::default(),
            price,
            amount,
        );
        let created_order = order.create_order(exchange.clone()).await.expect("in test");
        order
            .cancel_order_or_fail(&created_order, exchange.clone())
            .await;
    } else {
        panic!(
            "Incorrect currency pair setting enum type: {:?}",
            settings.strategy.currency_pair()
        );
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
