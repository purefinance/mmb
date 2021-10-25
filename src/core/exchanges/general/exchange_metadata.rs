use anyhow::{Context, Result};
use dashmap::DashMap;
use itertools::Itertools;
use log::warn;
use rust_decimal_macros::dec;
use std::sync::Arc;

use crate::core::exchanges::common::{CurrencyCode, CurrencyId, ExchangeAccountId};
use crate::core::infrastructure::WithExpect;
use crate::core::settings::CurrencyPairSetting;

use super::{currency_pair_metadata::CurrencyPairMetadata, exchange::Exchange};

impl Exchange {
    pub async fn build_metadata(&self, currency_pair_settings: &Option<Vec<CurrencyPairSetting>>) {
        let exchange_symbols = &self.request_metadata_with_retries().await;

        let supported_currencies = get_supported_currencies(exchange_symbols);
        self.setup_supported_currencies(supported_currencies);

        for metadata in exchange_symbols {
            self.leverage_by_currency_pair
                .insert(metadata.currency_pair(), dec!(1));
        }

        let currency_pairs = currency_pair_settings.with_expect(|| {
            format!(
                "Settings `currency_pairs` should be specified for exchange {}",
                self.exchange_account_id
            )
        });

        self.setup_symbols(get_symbols(
            &currency_pairs,
            exchange_symbols,
            self.exchange_account_id,
        ));
    }

    async fn request_metadata_with_retries(&self) -> Vec<Arc<CurrencyPairMetadata>> {
        const MAX_RETRIES: u8 = 5;
        let mut retry = 0;
        loop {
            match self.build_metadata_core().await {
                Ok(result_symbols) => return result_symbols,
                Err(error) => {
                    let error_message = format!(
                        "Unable to get metadata for {}: {:?}",
                        self.exchange_account_id, error
                    );

                    if retry < MAX_RETRIES {
                        warn!("{}", error_message);
                    } else {
                        panic!("{}", error_message);
                    }
                }
            }

            retry += 1;
        }
    }

    async fn build_metadata_core(&self) -> Result<Vec<Arc<CurrencyPairMetadata>>> {
        let response = &self.exchange_client.request_metadata().await?;

        if let Some(error) = self.get_rest_error(response) {
            Err(error).context("Rest error appeared during request request_metadata")?;
        }

        match self.exchange_client.parse_metadata(response) {
            symbols @ Ok(_) => symbols,
            Err(error) => {
                self.handle_parse_error(error, response, "".into(), None)?;
                Ok(Vec::new())
            }
        }
    }

    fn setup_supported_currencies(&self, supported_currencies: DashMap<CurrencyCode, CurrencyId>) {
        for (currency_code, currency_id) in supported_currencies {
            self.exchange_client
                .get_supported_currencies()
                .insert(currency_id, currency_code);
        }
    }

    fn setup_symbols(&self, symbols: Vec<Arc<CurrencyPairMetadata>>) {
        let mut currencies = symbols
            .iter()
            .flat_map(|x| vec![x.base_currency_code, x.quote_currency_code])
            .collect_vec();
        currencies.dedup();
        *self.currencies.lock() = currencies;

        symbols.iter().for_each(|symbol| {
            self.symbols.insert(symbol.currency_pair(), symbol.clone());
        });

        let exchange_client = &self.exchange_client;
        let current_specific_currencies = symbols
            .iter()
            .map(|x| exchange_client.get_specific_currency_pair(x.currency_pair()))
            .collect_vec();

        self.exchange_client
            .set_traded_specific_currencies(current_specific_currencies);
    }
}

fn get_supported_currencies(
    symbols: &[Arc<CurrencyPairMetadata>],
) -> DashMap<CurrencyCode, CurrencyId> {
    symbols
        .iter()
        .flat_map(|s| {
            [
                (s.base_currency_code, s.base_currency_id),
                (s.quote_currency_code, s.quote_currency_id),
            ]
        })
        .collect()
}

fn get_symbols(
    currency_pairs: &[CurrencyPairSetting],
    exchange_symbols: &[Arc<CurrencyPairMetadata>],
    exchange_account_id: ExchangeAccountId,
) -> Vec<Arc<CurrencyPairMetadata>> {
    currency_pairs
        .iter()
        .filter_map(|x| get_matched_currency_pair(x, exchange_symbols, exchange_account_id))
        .collect()
}

fn get_matched_currency_pair(
    currency_pair_setting: &CurrencyPairSetting,
    exchange_symbols: &[Arc<CurrencyPairMetadata>],
    exchange_account_id: ExchangeAccountId,
) -> Option<Arc<CurrencyPairMetadata>> {
    // currency pair metadata and currency pairs from settings should match 1 to 1
    let settings_currency_pair = currency_pair_setting.currency_pair.as_deref();
    let filtered_metadata = exchange_symbols
        .iter()
        .filter(|metadata| {
            return Some(metadata.currency_pair().as_str()) == settings_currency_pair
                || metadata.base_currency_code == currency_pair_setting.base
                    && metadata.quote_currency_code == currency_pair_setting.quote;
        })
        .take(2)
        .cloned()
        .collect_vec();

    match filtered_metadata.as_slice() {
        [] => {
            log::error!(
                "Unsupported symbol {:?} on exchange {}",
                currency_pair_setting,
                exchange_account_id
            );
        }
        [metadata] => return Some(metadata.clone()),
        _ => {
            log::error!(
                    "Found more then 1 symbol for currency pair {:?} on exchange {}. Found symbols: {:?}",
                    currency_pair_setting,
                    exchange_account_id,
                    filtered_metadata
                );
        }
    };

    None
}
