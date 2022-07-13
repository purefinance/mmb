use crate::config::Market;
use crate::services::liquidity::Amount;
use std::collections::HashMap;

#[derive(Clone)]
pub struct MarketInfo {
    pub desired_amount: Amount,
}

#[derive(Clone)]
pub struct MarketSettingsService {
    pub exchanges: HashMap<String, HashMap<String, MarketInfo>>,
}

impl From<Vec<Market>> for MarketSettingsService {
    fn from(markets: Vec<Market>) -> Self {
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
        Self { exchanges }
    }
}

impl MarketSettingsService {
    pub fn get_desired_amount(&self, exchange_id: &str, currency_pair: &str) -> Option<Amount> {
        match self.exchanges.get(exchange_id) {
            None => None,
            Some(exchange) => exchange.get(currency_pair).map(|info| info.desired_amount),
        }
    }
}
