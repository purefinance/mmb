use anyhow::Result;
use mmb_lib::core::settings::BaseStrategySettings;
use mmb_lib::core::{
    config::CONFIG_PATH,
    config::CREDENTIALS_PATH,
    exchanges::common::{Amount, CurrencyPair, ExchangeAccountId},
    lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings},
};
use mmb_lib::strategies::disposition_strategy::ExampleStrategy;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ExampleStrategySettings {}

impl BaseStrategySettings for ExampleStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance0"
            .parse()
            .expect("Binance should be specified for example strategy")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes(&"eos".into(), &"btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[allow(dead_code)]
#[actix_web::main]
async fn main() -> Result<()> {
    let engine_config = EngineBuildConfig::standard();

    let init_settings = InitSettings::<ExampleStrategySettings>::Load(
        CONFIG_PATH.to_owned(),
        CREDENTIALS_PATH.to_owned(),
    );

    let engine = launch_trading_engine(&engine_config, init_settings, |settings, ctx| {
        Box::new(ExampleStrategy::new(
            settings.strategy.exchange_account_id().clone(),
            settings.strategy.currency_pair().clone(),
            ctx,
        ))
    })
    .await?;

    // let ctx = engine.context();
    // let _ = tokio::spawn(async move {
    //     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    //     ctx.application_manager
    //         .clone()
    //         .spawn_graceful_shutdown("test".to_owned());
    // });

    engine.run().await;

    Ok(())
}
