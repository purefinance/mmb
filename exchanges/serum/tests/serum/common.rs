use anyhow::{Context, Result};
use mmb_core::exchanges::common::ExchangeAccountId;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory;
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use mmb_utils::hashmap;
use std::sync::Arc;

pub fn get_key_pair() -> Result<String> {
    std::env::var("SOLANA_KEY_PAIR").with_context(|| {
        "Environment variable SOLANA_KEY_PAIR are not set. Unable to continue test"
    })
}

pub fn get_timeout_manager(exchange_account_id: ExchangeAccountId) -> Arc<TimeoutManager> {
    let exchange_client = Box::new(serum::serum::SerumBuilder) as Box<dyn ExchangeClientBuilder>;
    let timeout_arguments = exchange_client.get_timeout_arguments();
    let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
        timeout_arguments,
        exchange_account_id,
    );

    TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
}
