use anyhow::{bail, Context, Result};
use mmb_core::exchanges::common::ExchangeAccountId;
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory;
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use mmb_utils::hashmap;
use serum::solana_client::{NetworkType, SolanaHosts};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub fn get_key_pair() -> Result<String> {
    get_key_pair_impl("SOLANA_KEY_PAIR")
}

pub fn get_additional_key_pair() -> Result<String> {
    get_key_pair_impl("SOLANA_ADDITIONAL_KEY_PAIR")
}

fn get_key_pair_impl(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| {
        format!("Environment variable {name} are not set. Unable to continue test")
    })
}

pub fn get_network_type() -> Result<NetworkType> {
    let markets_json = get_key_pair_impl("SERUM_MARKET_LIST")?;

    Ok(NetworkType::Custom(SolanaHosts::new(
        "https://api.devnet.solana.com".to_string(),
        "ws://api.devnet.solana.com/".to_string(),
        "".to_string(),
        Some(markets_json),
    )))
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

pub async fn retry_action<Out, Fut>(
    retry_count: u32,
    sleep_duration: Duration,
    action_name: &str,
    mut action: impl FnMut() -> Fut + Sized,
) -> Result<Out>
where
    Fut: Future<Output = Result<Out>>,
{
    let mut retry = 1;
    let error = loop {
        match action().await {
            Ok(out) => return Ok(out),
            Err(err) => {
                if retry < retry_count {
                    log::warn!("Error during action {action_name} on retry {retry}: {err:?}");
                    sleep(sleep_duration).await;
                    retry += 1;
                } else {
                    break err;
                }
            }
        }
    };

    bail!("Action {action_name} failed after {retry_count} retries. Last error: {error:?}");
}
