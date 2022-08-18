use crate::ws::subscribes::Subscription;
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, PartialEq, Eq, Hash, Deserialize, Debug, Default)]
pub struct BalancesSubscription;

impl Subscription for BalancesSubscription {
    fn get_hash(&self) -> u64 {
        let mut s = DefaultHasher::new();
        "balancesSubscription".hash(&mut s);
        s.finish()
    }
}
