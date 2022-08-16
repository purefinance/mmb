use dashmap::DashMap;
use itertools::Itertools;
use mmb_utils::infrastructure::WithExpect;
use rust_decimal_macros::dec;
use std::sync::Arc;

use crate::exchanges::common::{CurrencyCode, CurrencyId, ExchangeAccountId};
use crate::settings::CurrencyPairSetting;

use super::{exchange::Exchange, symbol::Symbol};

impl Exchange {
    pub async fn build_symbols(&self, currency_pair_settings: &Option<Vec<CurrencyPairSetting>>) {
        let exchange_symbols = &self.request_symbols_with_retries().await;

        let supported_currencies = get_supported_currencies(exchange_symbols);
        self.setup_supported_currencies(supported_currencies);

        for symbol in exchange_symbols {
            self.leverage_by_currency_pair
                .insert(symbol.currency_pair(), dec!(1));
        }

        let currency_pairs = currency_pair_settings.as_ref().with_expect(|| {
            format!(
                "Settings `currency_pairs` should be specified for exchange {}",
                self.exchange_account_id
            )
        });

        self.setup_symbols(get_symbols(
            currency_pairs,
            exchange_symbols,
            self.exchange_account_id,
        ));
    }

    async fn request_symbols_with_retries(&self) -> Vec<Arc<Symbol>> {
        const MAX_RETRIES: u8 = 5;
        for retry in 0..=MAX_RETRIES {
            match self.exchange_client.build_all_symbols().await {
                Ok(result_symbols) => return result_symbols,
                Err(error) => {
                    let error_message = format!(
                        "Unable to get symbol for {}: {error:?}",
                        self.exchange_account_id
                    );

                    if retry < MAX_RETRIES {
                        log::warn!("{error_message}");
                    } else {
                        panic!("{error_message}");
                    }
                }
            }
        }

        unreachable!()
    }

    fn setup_supported_currencies(&self, supported_currencies: DashMap<CurrencyCode, CurrencyId>) {
        for (currency_code, currency_id) in supported_currencies {
            self.exchange_client
                .get_supported_currencies()
                .insert(currency_id, currency_code);
        }
    }

    fn setup_symbols(&self, symbols: Vec<Arc<Symbol>>) {
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

fn get_supported_currencies(symbols: &[Arc<Symbol>]) -> DashMap<CurrencyCode, CurrencyId> {
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
    exchange_symbols: &[Arc<Symbol>],
    exchange_account_id: ExchangeAccountId,
) -> Vec<Arc<Symbol>> {
    currency_pairs
        .iter()
        .filter_map(|x| get_matched_currency_pair(x, exchange_symbols, exchange_account_id))
        .collect()
}

fn get_matched_currency_pair(
    currency_pair_setting: &CurrencyPairSetting,
    exchange_symbols: &[Arc<Symbol>],
    exchange_account_id: ExchangeAccountId,
) -> Option<Arc<Symbol>> {
    // currency pair symbol and currency pairs from settings should match 1 to 1
    let filtered_symbol = exchange_symbols
        .iter()
        .filter(|symbol| match currency_pair_setting {
            CurrencyPairSetting::Specific(currency_pair) => {
                symbol.currency_pair().as_str() == currency_pair
            }
            CurrencyPairSetting::Ordinary { base, quote } => {
                symbol.base_currency_code == *base && symbol.quote_currency_code == *quote
            }
        })
        .take(2)
        .cloned()
        .collect_vec();

    match filtered_symbol.as_slice() {
        [] => log::error!("Unsupported symbol {currency_pair_setting:?} on exchange {exchange_account_id}"),
        [symbol] => return Some(symbol.clone()),
        _ => log::error!("Found more then 1 symbol for currency pair {currency_pair_setting:?} on exchange {exchange_account_id}. Found symbols: {filtered_symbol:?}"),
    };

    None
}
