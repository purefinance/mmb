use std::sync::Arc;

use mmb_lib::{
    core::exchanges::common::ExchangeId,
    core::exchanges::{
        common::ExchangeAccountId,
        timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory,
        timeouts::timeout_manager::TimeoutManager,
    },
    core::lifecycle::launcher::EngineBuildConfig,
    hashmap,
};

// Get data to access binance account
#[macro_export]
macro_rules! get_binance_credentials_or_exit {
    () => {{
        let api_key = std::env::var("BINANCE_API_KEY");
        let api_key = match api_key {
            Ok(v) => v,
            Err(_) => {
                dbg!("Environment variable BINANCE_API_KEY are not set. Unable to continue test");
                return;
            }
        };

        let secret_key = std::env::var("BINANCE_SECRET_KEY");
        let secret_key = match secret_key {
            Ok(v) => v,
            Err(_) => {
                dbg!("Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test");
                return;
            }
        };

        (api_key, secret_key)
    }};
}

pub(crate) fn get_timeout_manager(exchange_account_id: &ExchangeAccountId) -> Arc<TimeoutManager> {
    let engine_build_config = EngineBuildConfig::standard();
    let timeout_arguments = engine_build_config.supported_exchange_clients
        [&ExchangeId::new("binance".into())]
        .get_timeout_argments();
    let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
        timeout_arguments,
        exchange_account_id.clone(),
    );

    TimeoutManager::new(hashmap![exchange_account_id.clone() => request_timeout_manager])
}
