use anyhow::Result;
use mmb_lib::core::exchanges::common::{Amount, CurrencyPair, ExchangeAccountId};
use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_lib::core::lifecycle::launcher::{load_settings, save_settings};
use mmb_lib::core::settings::BaseStrategySettings;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ExampleStrategySettings {
    pub test_value: bool,
}

impl BaseStrategySettings for ExampleStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance0"
            .parse()
            .expect("Binance should be specified for example strategy")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("eos".into(), "btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[allow(dead_code)]
#[actix_web::main]
async fn main() -> Result<()> {
    let engine_config = EngineBuildConfig::standard();

    let init_settings = InitSettings::Load("config.toml".to_owned(), "credentials.toml".to_owned());
    // FIXME delete
    let settings = load_settings::<ExampleStrategySettings>("config.toml", "credentials.toml")?;

    save_settings(settings, "saved_config.toml", "saved_credentials.toml")?;

    let engine =
        launch_trading_engine::<ExampleStrategySettings>(&engine_config, init_settings).await?;

    //// let ctx = engine.context();
    //// let _ = tokio::spawn(async move {
    ////     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    ////     ctx.application_manager
    ////         .clone()
    ////         .spawn_graceful_shutdown("test".to_owned());
    //// });

    //engine.run().await;

    Ok(())
}
