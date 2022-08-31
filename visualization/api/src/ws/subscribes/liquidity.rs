use crate::types::{CurrencyPair, ExchangeId};
use crate::ws::subscribes::Subscription;
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LiquiditySubscription {
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
}

impl Subscription for LiquiditySubscription {
    fn get_hash(&self) -> u64 {
        let mut s = DefaultHasher::new();
        "liquiditySubscription".hash(&mut s);
        self.hash(&mut s);
        s.finish()
    }
}
