use std::sync::Arc;

use anyhow::Result;

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
        settings::Hosts,
    },
    hashmap,
};
use rust_decimal::{prelude::FromPrimitive, Decimal};
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

pub(crate) async fn get_minimal_price(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    api_key: &String,
) -> Decimal {
    // structs for parsing json
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    struct Filter {
        #[serde(rename = "filterType")]
        pub filter_type: String,
        #[serde(rename = "minPrice")]
        pub min_price: Option<Decimal>,
        #[serde(rename = "avgPriceMins")]
        pub multiplier: Option<i64>,
    }
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    struct Symbol {
        pub filters: Vec<Filter>,
    }
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    struct ExchangeInfo {
        pub symbols: Vec<Symbol>,
    }

    // requesting data
    let http_params = vec![("symbol".to_owned(), currency_pair.as_str().to_owned())];

    let rest_client = RestClient::new();

    let url_path = "/api/v3/exchangeInfo";
    let full_url = rest_client::build_uri(&hosts.rest_host, url_path, &http_params)
        .expect("build_uri is failed");

    let data = rest_client
        .get(full_url, &api_key)
        .await
        .expect("failed to request exchangeInfo")
        .content;

    // json parsing
    let value: ExchangeInfo = serde_json::from_str(data.as_str())
        .with_expect(|| format!("failed to deserialize data: {}", data));

    // getting min_price from ExchangeInfo
    value
        .symbols
        .into_iter()
        .map(|x| {
            let min_price = x
                .filters
                .iter()
                .find(|y| y.filter_type == "PRICE_FILTER")
                .expect("Failed to find 'PRICE_FILTER'")
                .min_price
                .expect("min_price is None");

            // binance accept only min * multiplier prices
            let multiplier = x
                .filters
                .into_iter()
                .find(|y| y.filter_type == "PERCENT_PRICE")
                .expect("Failed to find 'PERCENT_PRICE'")
                .multiplier
                .expect("multiplier is None");

            (min_price
                * Decimal::from_i64(multiplier)
                    .with_expect(|| format!("Failed to convert {} to Decimal", multiplier)))
            .normalize()
        })
        .next()
        .expect("Failed to get min_price")
}
