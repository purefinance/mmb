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

use mmb_core::lifecycle::app_lifetime_manager::ActionAfterGracefulShutdown;

use crate::serum::common::get_key_pair;
use crate::serum::serum_builder::ExchangeSerumBuilder;
use futures::FutureExt;
use mmb_core::config::parse_settings;
use mmb_core::infrastructure::spawn_future_ok;
use mmb_core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_core::settings::BaseStrategySettings;
use mmb_utils::infrastructure::SpawnFutureFlags;
use strategies::example_strategy::{ExampleStrategy, ExampleStrategySettings};

#[ignore = "only for manual testing"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn example() {
    let engine_config = EngineBuildConfig::new(vec![Box::new(ExchangeSerumBuilder)]);

    let secret_key = get_key_pair().expect("Error getting solana keypair");
    let credentials = format!("[Serum_0]\napi_key = \"serum\"\nsecret_key = \"{secret_key}\"");

    let settings =
        parse_settings::<ExampleStrategySettings>(include_str!("config.toml"), &credentials)
            .expect("Error loading initial settings");

    let init_settings = InitSettings::Directly(settings);
    loop {
        let engine =
            launch_trading_engine(&engine_config, init_settings.clone(), |settings, ctx| {
                spawn_future_ok(
                    "Events logging",
                    SpawnFutureFlags::STOP_BY_TOKEN | SpawnFutureFlags::DENY_CANCELLATION,
                    {
                        let ctx = ctx.clone();
                        async move {
                            let mut events_rx = ctx.clone().get_events_channel();
                            loop {
                                let event_res = events_rx.recv().await;
                                match event_res {
                                    Ok(event) => {
                                        println!("Event has been received: {:?}", event);
                                    }
                                    Err(err) => {
                                        println!("Error occurred: {:?}", err);
                                        break;
                                    }
                                };
                            }
                        }
                        .boxed()
                    },
                );

                Box::new(ExampleStrategy::new(
                    settings.strategy.exchange_account_id(),
                    settings.strategy.currency_pair(),
                    settings.strategy.spread,
                    settings.strategy.max_amount,
                    ctx,
                ))
            })
            .await
            .expect("Failed to launch trading engine");

        match engine.run().await {
            ActionAfterGracefulShutdown::Nothing => break,
            ActionAfterGracefulShutdown::Restart => continue,
        }
    }
}
