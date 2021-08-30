use crate::core::exchanges::common::{CurrencyCode, CurrencyId};
use crate::core::exchanges::general::exchange::Exchange;
use anyhow::{bail, Result};
use dashmap::DashMap;
use itertools::Itertools;
use log::warn;
use rust_decimal_macros::dec;
use std::sync::Arc;

use super::currency_pair_metadata::CurrencyPairMetadata;

impl Exchange {
    pub async fn build_metadata(&self) {
        let symbols;

        const MAX_RETRIES: u8 = 5;
        let mut retry = 0u8;
        loop {
            match self.build_metadata_core().await {
                Ok(result_symbols) => {
                    symbols = result_symbols;
                    break;
                }
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

        let supported_currencies = Self::get_supported_currencies(&symbols[..]);
        self.set_supported_currencies(supported_currencies);

        let currency_pairs = symbols.iter().map(|x| x.currency_pair()).collect_vec();
        for currency_pair in currency_pairs {
            self.leverage_by_currency_pair
                .insert(currency_pair, dec!(1));
        }
        *self.supported_symbols.lock() = symbols;
    }

    fn set_supported_currencies(&self, supported_currencies: DashMap<CurrencyCode, CurrencyId>) {
        for (currency_code, currency_id) in supported_currencies {
            self.exchange_client
                .get_supported_currencies()
                .insert(currency_id, currency_code);
        }
    }

    async fn build_metadata_core(&self) -> Result<Vec<Arc<CurrencyPairMetadata>>> {
        let response = self.exchange_client.request_metadata().await?;

        if let Some(error) = self.get_rest_error(&response) {
            bail!(
                "Rest error appeared during request request_metadata: {}",
                error.message
            );
        }

        match self.exchange_client.parse_metadata(&response) {
            symbols @ Ok(_) => symbols,
            Err(error) => {
                self.handle_parse_error(error, response, "".into(), None)?;
                Ok(Vec::new())
            }
        }
    }

    fn get_supported_currencies(
        symbols: &[Arc<CurrencyPairMetadata>],
    ) -> DashMap<CurrencyCode, CurrencyId> {
        symbols
            .iter()
            .flat_map(|s| {
                vec![
                    (s.base_currency_code.clone(), s.base_currency_id.clone()),
                    (s.quote_currency_code.clone(), s.quote_currency_id.clone()),
                ]
            })
            .collect()
    }

    pub fn set_symbols(&self, symbols: Vec<Arc<CurrencyPairMetadata>>) {
        let mut currencies = symbols
            .iter()
            .flat_map(|x| vec![x.base_currency_code.clone(), x.quote_currency_code.clone()])
            .collect_vec();
        currencies.dedup();
        *self.currencies.lock() = currencies;

        let mut current_specific_currencies = Vec::new();
        symbols.iter().for_each(|symbol| {
            self.symbols.insert(symbol.currency_pair(), symbol.clone());
            current_specific_currencies.push(
                self.exchange_client
                    .get_specific_currency_pair(&symbol.currency_pair()),
            );
        });
        self.exchange_client
            .set_traded_specific_currencies(current_specific_currencies);
    }
}
