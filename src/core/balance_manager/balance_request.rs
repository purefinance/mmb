use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
#[derive(Hash)]
pub(crate) struct BalanceRequest {
    pub configureation_descriptor: Arc<ConfigurationDescriptor>,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub currency_code: CurrencyCode,
}

impl BalanceRequest {
    pub fn new(
        configureation_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        currency_code: CurrencyCode,
    ) -> Self {
        Self {
            configureation_descriptor: configureation_descriptor,
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
