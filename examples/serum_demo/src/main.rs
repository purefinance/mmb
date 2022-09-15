#![deny(
    non_ascii_idents,
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

use mmb_core::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;
use serum::serum::SerumBuilder;

use mmb_core::config::{CONFIG_PATH, CREDENTIALS_PATH};
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::settings::BaseStrategySettings;

use strategies::example_strategy::{ExampleStrategy, ExampleStrategySettings};

#[tokio::main]
async fn main() {
    let engine_config = EngineBuildConfig::new(vec![Box::new(SerumBuilder)]);

    let init_settings = InitSettings::<ExampleStrategySettings>::Load {
        config_path: CONFIG_PATH.to_owned(),
        credentials_path: CREDENTIALS_PATH.to_owned(),
    };
    loop {
        let engine = launch_trading_engine(&engine_config, init_settings.clone())
            .await
            .expect("Failed to launch_trading_engine");

        let settings = engine.settings();
        let strategy = ExampleStrategy::new(
            settings.strategy.exchange_account_id(),
            settings.strategy.currency_pair(),
            settings.strategy.spread,
            settings.strategy.max_amount,
            engine.context(),
        );

        engine.start_disposition_executor(strategy);

        match engine.run().await {
            ActionAfterGracefulShutdown::Nothing => break,
            ActionAfterGracefulShutdown::Restart => continue,
        }
    }
}
