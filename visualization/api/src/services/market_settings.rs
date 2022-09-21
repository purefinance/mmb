use std::collections::HashMap;

use itertools::Itertools;
use serde_json::{json, Value};

use mmb_domain::order::snapshot::Amount;

use crate::config::Market;
use crate::types::{CurrencyPair, ExchangeId};

#[derive(Clone)]
pub struct MarketInfo {
    pub desired_amount: Amount,
}

#[derive(Clone)]
pub struct MarketSettingsService {
    pub exchanges: HashMap<String, HashMap<String, MarketInfo>>,
    pub supported_exchanges: Vec<Value>,
}

impl From<Vec<Market>> for MarketSettingsService {
    fn from(markets: Vec<Market>) -> Self {
        let supported_exchanges = markets
            .iter()
            .map(|it| {
                let symbols = it
                    .info
                    .iter()
                    .map(|it| {
                        json!({
                            "currencyCodePair": it.currency_pair,
                            "currencyPair": it.currency_pair.to_uppercase()
                        })
                    })
                    .collect_vec();
                json!({
                    "name": it.exchange_id,
                    "symbols": symbols
                })
            })
            .collect_vec();

        let mut exchanges = HashMap::new();
        for market in markets.into_iter() {
            let mut currency_pairs = HashMap::new();
            for info in market.info {
                currency_pairs.insert(
                    info.currency_pair,
                    MarketInfo {
                        desired_amount: info.max_amount,
                    },
                );
            }
            exchanges.insert(market.exchange_id, currency_pairs);
        }
        Self {
            exchanges,
            supported_exchanges,
        }
    }
}

impl MarketSettingsService {
    pub fn get_desired_amount(
        &self,
        exchange_id: &ExchangeId,
        currency_pair: &CurrencyPair,
    ) -> Option<Amount> {
        match self.exchanges.get(exchange_id) {
            None => None,
            Some(exchange) => exchange.get(currency_pair).map(|info| info.desired_amount),
        }
    }
}
