use anyhow::Result;
use binance::binance::BinanceBuilder;
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use mmb_core::config::{CONFIG_PATH, CREDENTIALS_PATH};
use mmb_core::exchanges::common::{Amount, CurrencyPair, ExchangeAccountId};
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::settings::{BaseStrategySettings, CurrencyPairSetting};

use example::strategies::example_strategy::ExampleStrategy;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ExampleStrategySettings {
    pub spread: Decimal,
    pub currency_pair: CurrencyPairSetting,
    pub max_amount: Decimal,
}

impl BaseStrategySettings for ExampleStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance_0"
            .parse()
            .expect("Binance should be specified for example strategy")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes(self.currency_pair.base, self.currency_pair.quote)
    }

    // Max amount for orders that will be created
    fn max_amount(&self) -> Amount {
        self.max_amount
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let engine_config =
        EngineBuildConfig::standard(Box::new(BinanceBuilder) as Box<dyn ExchangeClientBuilder>);

    let init_settings = InitSettings::<ExampleStrategySettings>::Load {
        config_path: CONFIG_PATH.to_owned(),
        credentials_path: CREDENTIALS_PATH.to_owned(),
    };

    let engine = launch_trading_engine(&engine_config, init_settings, |settings, ctx| {
        Box::new(ExampleStrategy::new(
            settings.strategy.exchange_account_id(),
            settings.strategy.currency_pair(),
            settings.strategy.spread,
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
