use anyhow::{bail, Result};
use bitmex::bitmex::{BitmexBuilder, ErrorHandlerBitmex, RestHeadersBitmex};
use bitmex::types::BitmexOrderBookInsert;
use itertools::Itertools;
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::rest_client::{ErrorHandlerData, RestClient, UriBuilder};
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory;
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::lifecycle::launcher::EngineBuildConfig;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::exchanges::symbol::Precision;
use mmb_domain::market::{CurrencyPair, ExchangeAccountId, SpecificCurrencyPair};
use mmb_domain::order::snapshot::{Amount, OrderSide, Price};
use mmb_utils::hashmap;
use mmb_utils::infrastructure::WithExpect;
use rust_decimal_macros::dec;
use std::sync::Arc;

pub(crate) fn default_currency_pair() -> CurrencyPair {
    CurrencyPair::from_codes("xbt".into(), "usd".into())
}

/// Returns tuple of execution_price (order with such price supposed to be executed immediately)
/// and min_price (for orders which must be opened after creation for a some time)
pub(crate) async fn get_prices(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    settings: &ExchangeSettings,
    price_precision: &Precision,
) -> (Price, Price) {
    let mut builder = UriBuilder::from_path("/api/v1/orderBook/L2");
    builder.add_kv("symbol", currency_pair);
    builder.add_kv("depth", 25);
    let uri = builder.build_uri(hosts.rest_uri_host(), true);

    let rest_client = RestClient::new(
        ErrorHandlerData::new(
            false,
            settings.exchange_account_id,
            ErrorHandlerBitmex::default(),
        ),
        RestHeadersBitmex::new(settings.api_key.clone(), settings.secret_key.clone()),
    );

    let data = rest_client
        .get(uri, "get_default_price()", "".to_string())
        .await
        .expect("Failed to request order book")
        .content;

    let order_book: Vec<BitmexOrderBookInsert> =
        serde_json::from_str(&data).with_expect(|| format!("failed to deserialize data: {data}"));

    let top_bid_price = order_book
        .iter()
        .find_or_first(|record| record.side == OrderSide::Buy)
        .expect("Can't get bid value from order book")
        .price;

    let low_bid_price = order_book
        .iter()
        .find_or_last(|record| record.side == OrderSide::Buy)
        .expect("Can't get bid value from order book")
        .price;

    (
        top_bid_price + price_precision.get_tick() * dec!(2),
        low_bid_price,
    )
}

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

pub(crate) fn get_position_value_by_side(side: OrderSide, position: Amount) -> Amount {
    match side {
        OrderSide::Buy => position,
        OrderSide::Sell => -position,
    }
}
