use std::hash::Hash;
use std::sync::Arc;

use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

#[derive(Hash, Debug, PartialEq)]
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
            configuration_descriptor,
            exchange_account_id,
            currency_pair,
            currency_code,
        }
    }
}

impl Eq for BalanceRequest {}
