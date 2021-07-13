#![cfg(test)]
use anyhow::Result;
use futures::FutureExt;
use mmb_lib::core::config::parse_settings;
use mmb_lib::core::disposition_execution::{PriceSlot, TradingContext};
use mmb_lib::core::explanation::Explanation;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::order_book::local_snapshot_service::LocalSnapshotsService;
use mmb_lib::core::orders::order::OrderSnapshot;
use mmb_lib::core::settings::BaseStrategySettings;
use mmb_lib::core::{
    exchanges::common::Amount,
    lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings},
    DateTime,
};
use mmb_lib::core::{
    exchanges::common::{CurrencyPair, ExchangeAccountId},
    infrastructure::spawn_future,
};
use mmb_lib::strategies::disposition_strategy::DispositionStrategy;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

impl BaseStrategySettings for TestStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance0".parse().expect("for testing")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("eth".into(), "btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[actix_rt::test]
async fn launch_engine() -> Result<()> {
    struct TestStrategy;

    impl DispositionStrategy for TestStrategy {
        fn calculate_trading_context(
            &mut self,
            _max_amount: Decimal,
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
            _target_eai: &ExchangeAccountId,
            _cancellation_token: CancellationToken,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let config = EngineBuildConfig::standard();

    let settings = parse_settings::<TestStrategySettings>(
        include_str!("lifecycle.toml"),
        include_str!("lifecycle.cred.toml"),
    )?;
    let init_settings = InitSettings::Directly(settings);
    let engine = launch_trading_engine(&config, init_settings, |_| Box::new(TestStrategy)).await?;

    let context = engine.context();

    let action = async move {
        sleep(Duration::from_millis(200)).await;
        context
            .application_manager
            .run_graceful_shutdown("test")
            .await;

        Ok(())
    };
    spawn_future(
        "run graceful_shutdown in launch_engine test",
        true,
        action.boxed(),
    );

    engine.run().await;

    Ok(())
}
