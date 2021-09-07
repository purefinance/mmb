use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
#[derive(Hash, Debug)]
pub struct BalanceRequest {
    pub configuration_descriptor: Arc<ConfigurationDescriptor>,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub currency_code: CurrencyCode,
}

impl BalanceRequest {
    pub fn new(
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        currency_code: CurrencyCode,
    ) -> Self {
        Self {
            configuration_descriptor: configuration_descriptor,
            exchange_account_id: exchange_account_id,
            currency_pair: currency_pair,
            currency_code: currency_code,
        }
    }

    pub fn get_hash_code(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl PartialEq for BalanceRequest {
    fn eq(&self, other: &Self) -> bool {
        self.configuration_descriptor == other.configuration_descriptor
            && self.exchange_account_id == other.exchange_account_id
            && self.currency_pair == other.currency_pair
            && self.currency_code == other.currency_code
    }
}

impl Eq for BalanceRequest {}
