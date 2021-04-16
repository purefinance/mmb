use crate::core::exchanges::common::{CurrencyCode, CurrencyId};
use crate::core::exchanges::general::exchange::Exchange;
use anyhow::{bail, Result};
use dashmap::DashMap;
use itertools::Itertools;
use log::warn;
use std::sync::Arc;

use super::currency_pair_metadata::{
    CurrencyPairMetadata, CURRENCY_PAIR_METADATA_DEFAULT_PRECISION,
};

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
                    if retry < MAX_RETRIES {
                        warn!(
                            "We got empty metadata for {} with error: {:?}",
                            self.exchange_account_id, error
                        );
                    } else {
                        panic!("We got empty metadata for {}", self.exchange_account_id);
                    }
                }
            }

            retry += 1;
        }

        let supported_symbols = symbols
            .into_iter()
            .filter(|s| {
                s.amount_precision != CURRENCY_PAIR_METADATA_DEFAULT_PRECISION
                    && s.price_precision != CURRENCY_PAIR_METADATA_DEFAULT_PRECISION
            })
            .collect_vec();

        let supported_currencies = Self::get_supported_currencies(&supported_symbols[..]);
        self.set_supported_currencies(supported_currencies);

        *self.supported_symbols.lock() = supported_symbols;
    }

    fn set_supported_currencies(&self, supported_currencies: DashMap<CurrencyCode, CurrencyId>) {
        for (currency_code, currency_id) in supported_currencies {
            self.supported_currencies.insert(currency_code, currency_id);
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
            symbols @ Ok(_) => {
                return symbols;
            }
            Err(error) => {
                self.handle_parse_error(error, response, "".into(), None)?;
                return Ok(Vec::new());
            }
        };
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

        symbols.iter().for_each(|symbol| {
            self.symbols.insert(symbol.currency_pair(), symbol.clone());
        });
    }
}
