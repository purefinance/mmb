use anyhow::Result;
use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_lib::core::settings::BaseStrategySettings;
use mmb_lib::core::{
    config::load_settings,
    config::update_settings,
    exchanges::common::{Amount, CurrencyPair, ExchangeAccountId},
};
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

    let engine =
        launch_trading_engine::<ExampleStrategySettings>(&engine_config, init_settings).await?;

    // FIXME delete
    let settings = r#"
            [strategy]

            [core]
            [[core.exchanges]]
            exchange_account_id = "Binance0"
            is_margin_trading = false
            api_key = "wow"
            secret_key = "mom"
            web_socket_host = ""
            web_socket2_host = ""
            rest_host = ""
            websocket_channels = [ "depth20" ]
            subscribe_to_market_data = true

            currency_pairs = [ { base = "phb", quote = "btc" },
                               { base = "eth", quote = "btc" },
                               { base = "eos", quote = "btc" } ]
        "#;
    update_settings(settings, "saved_config.toml", "saved_credentials.toml")?;

    // let ctx = engine.context();
    // let _ = tokio::spawn(async move {
    //     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    //     ctx.application_manager
    //         .clone()
    //         .spawn_graceful_shutdown("test".to_owned());
    // });

    //engine.run().await;

    Ok(())
}
