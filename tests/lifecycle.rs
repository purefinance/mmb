#![cfg(test)]
use anyhow::Result;
use futures::FutureExt;
use mmb_lib::core::settings::{AppSettings, BaseStrategySettings};
use mmb_lib::core::{
    exchanges::common::Amount,
    lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings},
};
use mmb_lib::core::{
    exchanges::common::{CurrencyPair, ExchangeAccountId},
    infrastructure::spawn_future,
};
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct TestStrategySettings {}

impl BaseStrategySettings for TestStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "TestExchange0".parse().expect("for testing")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("base".into(), "quote".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[actix_rt::test]
async fn launch_engine() -> Result<()> {
    let config = EngineBuildConfig::standard();

    let init_settings = InitSettings::Directly(AppSettings::<TestStrategySettings>::default());
    let engine = launch_trading_engine(&config, init_settings).await?;

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
