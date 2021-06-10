use crate::core::lifecycle::launcher::EngineBuildConfig;
use crate::core::settings::{CurrencyPairSetting, ExchangeSettings};
use crate::core::{
    exchanges::{
        general::exchange::Exchange,
        timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory,
        timeouts::timeout_manager::TimeoutManager,
    },
    settings::CoreSettings,
};
use itertools::Itertools;
use log::error;
use std::sync::mpsc::channel;
use std::sync::Arc;

use super::{commission::Commission, currency_pair_metadata::CurrencyPairMetadata};

pub fn create_timeout_manager(
    core_settings: &CoreSettings,
    build_settings: &EngineBuildConfig,
) -> Arc<TimeoutManager> {
    let request_timeout_managers = core_settings
        .exchanges
        .iter()
        .map(|exchange_settings| {
            let timeout_arguments = build_settings.supported_exchange_clients
                [&exchange_settings.exchange_account_id.exchange_id]
                .get_timeout_argments();

            let exchange_account_id = exchange_settings.exchange_account_id.clone();
            let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
                timeout_arguments,
                exchange_account_id.clone(),
            );

            (exchange_account_id, request_timeout_manager)
        })
        .collect();

    TimeoutManager::new(request_timeout_managers)
}

pub async fn create_exchange(
    exchange_settings: &ExchangeSettings,
    build_settings: &EngineBuildConfig,
    timeout_manager: Arc<TimeoutManager>,
) -> Arc<Exchange> {
    let (exchange_client, features) = build_settings.supported_exchange_clients
        [&exchange_settings.exchange_account_id.exchange_id]
        .create_exchange_client(exchange_settings.clone());

    let (tx, _) = channel();
    let exchange = Exchange::new(
        exchange_settings.exchange_account_id.clone(),
        exchange_settings.web_socket_host.clone(),
        vec![],
        exchange_settings.websocket_channels.clone(),
        exchange_client,
        features,
        tx,
        timeout_manager.clone(),
        Commission::default(),
    );

    exchange.build_metadata().await;

    if let Some(currency_pairs) = &exchange_settings.currency_pairs {
        exchange.set_symbols(get_symbols(&exchange, &currency_pairs[..]))
    }

    exchange
}

pub fn get_symbols(
    exchange: &Arc<Exchange>,
    currency_pairs: &[CurrencyPairSetting],
) -> Vec<Arc<CurrencyPairMetadata>> {
    let mut symbols = Vec::new();

    let supported_symbols_guard = exchange.supported_symbols.lock();
    for currency_pair_setting in currency_pairs {
        let mut filtered_symbols = supported_symbols_guard
            .iter()
            .filter(|x| {
                if let Some(currency_pair) = &currency_pair_setting.currency_pair {
                    return currency_pair.as_str() == x.currency_pair().as_str();
                }

                return x.base_currency_code == currency_pair_setting.base
                    && x.quote_currency_code == currency_pair_setting.quote;
            })
            .take(2)
            .collect_vec();

        let symbol = match filtered_symbols.len() {
            0 => {
                error!(
                    "Unsupported symbol {:?} on exchange {}",
                    currency_pair_setting, exchange.exchange_account_id
                );
                continue;
            }
            1 => filtered_symbols
                .pop()
                .expect("we checked already that 1 symbol found"),
            _ => {
                error!(
                    "Found more then 1 symbol for currency pair {:?}. Found symbols: {:?}",
                    currency_pair_setting, filtered_symbols
                );
                continue;
            }
        };

        symbols.push(symbol.clone());
    }

    symbols
}
