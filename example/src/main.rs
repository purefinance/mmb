#![deny(
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_must_use,
    clippy::unwrap_used
)]

mod strategies;

use anyhow::{anyhow, Result};
use binance::binance::BinanceBuilder;
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use mmb_core::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use mmb_core::config::{CONFIG_PATH, CREDENTIALS_PATH};
use mmb_core::exchanges::common::{Amount, CurrencyPair, ExchangeAccountId};
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::settings::{BaseStrategySettings, CurrencyPairSetting};

use crate::strategies::example_strategy::ExampleStrategy;

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
        if let CurrencyPairSetting::Ordinary { base, quote } = self.currency_pair {
            CurrencyPair::from_codes(base, quote)
        } else {
            panic!(
                "Incorrect currency pair setting enum type {:?}",
                self.currency_pair
            );
        }
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
    loop {
        let engine =
            launch_trading_engine(&engine_config, init_settings.clone(), |settings, ctx| {
                Box::new(ExampleStrategy::new(
                    settings.strategy.exchange_account_id(),
                    settings.strategy.currency_pair(),
                    settings.strategy.spread,
                    settings.strategy.max_amount,
                    ctx,
                ))
            })
            .await?
            .ok_or_else(|| anyhow!("Failed to launch_trading_engine"))?;

        match engine.run().await {
            ActionAfterGracefulShutdown::Nothing => break,
            ActionAfterGracefulShutdown::Restart => continue,
        }
    }
    Ok(())
}
