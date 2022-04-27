#![cfg(test)]
use binance::binance::BinanceBuilder;
use mmb_core::config::parse_settings;
use mmb_core::disposition_execution::{PriceSlot, TradingContext};
use mmb_core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use mmb_core::explanation::Explanation;
use mmb_core::infrastructure::spawn_future_ok;
use mmb_core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_core::orders::order::OrderSnapshot;
use mmb_core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use mmb_core::settings::BaseStrategySettings;
use mmb_core::strategies::disposition_strategy::DispositionStrategy;
use mmb_core::{
    exchanges::common::Amount,
    lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings},
};
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::infrastructure::SpawnFutureFlags;
use mmb_utils::DateTime;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

impl BaseStrategySettings for TestStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance_0".parse().expect("for testing")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("eth".into(), "btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn launch_engine() -> anyhow::Result<()> {
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
            ConfigurationDescriptor::new("TestStrategy".into(), "lifecycle_test".into())
        }
    }

    let config =
        EngineBuildConfig::standard(Box::new(BinanceBuilder) as Box<dyn ExchangeClientBuilder>);

    let settings = match parse_settings::<TestStrategySettings>(
        include_str!("lifecycle.toml"),
        include_str!("lifecycle.cred.toml"),
    ) {
        Ok(settings) => settings,
        Err(_) => return Ok(()), // For CI, while we cant setup keys on github
    };

    let init_settings = InitSettings::Directly(settings);
    let engine = launch_trading_engine(&config, init_settings, |_, _| Box::new(TestStrategy))
        .await?
        .expect("Failed to launch TradingEngine");

    let context = engine.context();

    let action = async move {
        sleep(Duration::from_millis(200)).await;
        context.lifetime_manager.run_graceful_shutdown("test").await;
    };
    spawn_future_ok(
        "run graceful_shutdown in launch_engine test",
        SpawnFutureFlags::DENY_CANCELLATION,
        action,
    );

    engine.run().await;

    Ok(())
}
