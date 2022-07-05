use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LiquiditySubscription {
    pub exchange_id: String,
    pub currency_pair: String,
}

pub trait Subscription {
    // The hash of the subscription is intended to target messages only to those users who have this subscription.
    fn get_hash(&self) -> u64;
}

impl Subscription for LiquiditySubscription {
    fn get_hash(&self) -> u64 {
        let mut s = DefaultHasher::new();
        "liquiditySubscription".hash(&mut s);
        self.hash(&mut s);
        s.finish()
    }
}
