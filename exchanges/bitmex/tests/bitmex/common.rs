use anyhow::{bail, Result};
use bitmex::bitmex::BitmexBuilder;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory;
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::lifecycle::launcher::EngineBuildConfig;
use mmb_domain::market::ExchangeAccountId;
use mmb_utils::hashmap;
use std::sync::Arc;

pub(crate) fn get_timeout_manager(exchange_account_id: ExchangeAccountId) -> Arc<TimeoutManager> {
    let engine_build_config = EngineBuildConfig::new(vec![Box::new(BitmexBuilder)]);
    let timeout_arguments = engine_build_config.supported_exchange_clients
        [&exchange_account_id.exchange_id]
        .get_timeout_arguments();

    let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
        timeout_arguments,
        exchange_account_id,
    );

    TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
}

pub(crate) fn get_bitmex_credentials() -> Result<(String, String)> {
    let api_key = std::env::var("BITMEX_API_KEY");
    let api_key = match api_key {
        Ok(v) => v,
        Err(_) => {
            bail!("Environment variable BITMEX_API_KEY are not set. Unable to continue test",)
        }
    };

    let secret_key = std::env::var("BITMEX_SECRET_KEY");
    let secret_key = match secret_key {
        Ok(v) => v,
        Err(_) => {
            bail!("Environment variable BITMEX_SECRET_KEY are not set. Unable to continue test",)
        }
    };

    Ok((api_key, secret_key))
}
