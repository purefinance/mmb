use std::sync::Arc;

use anyhow::Result;

use mmb_lib::core::exchanges::hosts::Hosts;
use mmb_lib::{
    core::exchanges::common::ExchangeId,
    core::exchanges::{
        common::ExchangeAccountId,
        timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory,
        timeouts::timeout_manager::TimeoutManager,
    },
    core::{
        exchanges::{
            common::SpecificCurrencyPair,
            rest_client::{self, RestClient},
        },
        infrastructure::WithExpect,
        lifecycle::launcher::EngineBuildConfig,
    },
    hashmap,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub(crate) fn get_binance_credentials() -> Result<(String, String)> {
    let api_key = std::env::var("BINANCE_API_KEY");
    let api_key = match api_key {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow::Error::msg(
                "Environment variable BINANCE_API_KEY are not set. Unable to continue test",
            ));
        }
    };

    let secret_key = std::env::var("BINANCE_SECRET_KEY");
    let secret_key = match secret_key {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow::Error::msg(
                "Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test",
            ));
        }
    };

    Ok((api_key, secret_key))
}

// Get data to access binance account
#[macro_export]
macro_rules! get_binance_credentials_or_exit {
    () => {{
        match crate::binance::common::get_binance_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(error) => {
                dbg!("{:?}", error);
                return;
            }
        }
    }};
}

pub(crate) fn get_timeout_manager(exchange_account_id: ExchangeAccountId) -> Arc<TimeoutManager> {
    let engine_build_config = EngineBuildConfig::standard();
    let timeout_arguments = engine_build_config.supported_exchange_clients
        [&ExchangeId::new("Binance".into())]
        .get_timeout_arguments();
    let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
        timeout_arguments,
        exchange_account_id,
    );

    TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
}

/// Automatic price calculation for orders. This function gets the price from the middle of order book bids side.
/// This helps to avoid creating orders in the top of the order book.
pub(crate) async fn get_default_price(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    api_key: &String,
) -> Decimal {
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    struct OrderBook {
        pub bids: Vec<(Decimal, Decimal)>,
    }

    let http_params = vec![("symbol".to_owned(), currency_pair.as_str().to_owned())];

    let rest_client = RestClient::new();

    let url_path = "/api/v3/depth";
    let full_url = rest_client::build_uri(&hosts.rest_host, url_path, &http_params)
        .expect("build_uri is failed");

    let data = rest_client
        .get(full_url, &api_key)
        .await
        .expect("failed to request exchangeInfo")
        .content;

    let value: OrderBook = serde_json::from_str(data.as_str())
        .with_expect(|| format!("failed to deserialize data: {}", data));

    // getting price for order from the middle of the order book
    // use bids because this price is little lower then asks
    value
        .bids
        .get(value.bids.len() / 2)
        .expect("failed to get bid from the middle of the order book")
        .clone()
        .0
}
