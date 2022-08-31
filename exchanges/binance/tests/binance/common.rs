use anyhow::Result;
use binance::binance::{BinanceBuilder, ErrorHandlerBinance};
use domain::exchanges::symbol::{Round, Symbol};
use domain::market::{ExchangeAccountId, SpecificCurrencyPair};
use domain::order::snapshot::{Amount, Price};
use function_name::named;
use hyper::Uri;
use jsonrpc_core::Value;
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::rest_client::{ErrorHandlerData, UriBuilder};
use mmb_core::{
    exchanges::rest_client::RestClient,
    exchanges::{
        timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory,
        timeouts::timeout_manager::TimeoutManager,
    },
    lifecycle::launcher::EngineBuildConfig,
};
use mmb_utils::hashmap;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::value_to_decimal::GetOrErr;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
        match $crate::binance::common::get_binance_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(_) => {
                return;
            }
        }
    }};
}

pub(crate) fn get_timeout_manager(exchange_account_id: ExchangeAccountId) -> Arc<TimeoutManager> {
    let engine_build_config = EngineBuildConfig::new(vec![Box::new(BinanceBuilder)]);
    let timeout_arguments = engine_build_config.supported_exchange_clients
        [&exchange_account_id.exchange_id]
        .get_timeout_arguments();

    let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
        timeout_arguments,
        exchange_account_id,
    );

    TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
}

#[named]
async fn send_request(uri: Uri, api_key: &str, exchange_account_id: ExchangeAccountId) -> String {
    let rest_client = RestClient::new(ErrorHandlerData::new(
        false,
        exchange_account_id,
        ErrorHandlerBinance::default(),
    ));

    rest_client
        .get(uri.clone(), api_key, function_name!(), "".to_string())
        .await
        .with_expect(|| format!("failed to request {uri}"))
        .content
}

/// Automatic price calculation for orders. This function gets the price from 10-th price level of
/// order book if it exists otherwise last bid price from order book.
/// This helps to avoid creating order in the top of the order book and filling it.
pub(crate) async fn get_default_price(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    api_key: &str,
    exchange_account_id: ExchangeAccountId,
    is_margin_trading: bool,
) -> Price {
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    struct OrderBook {
        pub bids: Vec<(Decimal, Decimal)>,
    }

    let mut builder = UriBuilder::from_path(match is_margin_trading {
        true => "/fapi/v1/depth",
        false => "/api/v3/depth",
    });
    builder.add_kv("symbol", &currency_pair);
    builder.add_kv("limit", "20");
    let uri = builder.build_uri(hosts.rest_uri_host(), true);

    let data = send_request(uri, api_key, exchange_account_id).await;

    let value: OrderBook =
        serde_json::from_str(&data).with_expect(|| format!("failed to deserialize data: {data}"));

    let last = value.bids.iter().last();
    last.with_expect(|| format!("can't get bid from {currency_pair} orderbook because it's empty"))
        .0
}

/// Automatic amount calculation for orders. This function calculate the amount for price and MIN_NOTIONAL filter.
pub(crate) async fn get_min_amount(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    api_key: &str,
    price: Price,
    symbol: &Symbol,
    exchange_account_id: ExchangeAccountId,
    is_margin_trading: bool,
) -> Amount {
    let mut builder = UriBuilder::from_path(match is_margin_trading {
        true => "/fapi/v1/exchangeInfo",
        false => "/api/v3/exchangeInfo",
    });
    builder.add_kv("symbol", &currency_pair);
    let uri = builder.build_uri(hosts.rest_uri_host(), true);

    let data = send_request(uri, api_key, exchange_account_id).await;

    let value: Value =
        serde_json::from_str(&data).with_expect(|| format!("failed to deserialize data: {data}"));

    let filters = value
        .pointer("/symbols/0/filters")
        .expect("Failed to get filters")
        .as_array()
        .expect("/symbols/0/filters isn't an array");

    let min_notional_filter = filters
        .iter()
        .find(|value| {
            value["filterType"]
                .as_str()
                .expect("Failed to get filterType")
                == "MIN_NOTIONAL"
        })
        .expect("Failed to get min_notional_filter");

    let min_notional = min_notional_filter
        .get_as_decimal(match is_margin_trading {
            true => "notional",
            false => "minNotional",
        })
        .expect("Failed to get min_notional");

    symbol.amount_round(min_notional / price, Round::Ceiling)
}
