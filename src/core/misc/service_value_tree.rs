use std::collections::HashMap;
use std::sync::Arc;

use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

use itertools::Itertools;
use rust_decimal::Decimal;

pub(crate) type ServiceNameConfigurationKeyMap = HashMap<String, ConfigurationKeyExchangeIdMap>;
pub(crate) type ConfigurationKeyExchangeIdMap =
    HashMap<String, ExchangeAccountIdCurrencyCodePairMap>;
pub(crate) type ExchangeAccountIdCurrencyCodePairMap =
    HashMap<ExchangeAccountId, CurrencyPairCurrencyCodeMap>;
pub(crate) type CurrencyPairCurrencyCodeMap = HashMap<CurrencyPair, CurrencyCodeValueMap>;
pub(crate) type CurrencyCodeValueMap = HashMap<CurrencyCode, Decimal>;

pub(crate) struct ServiceValueTree {
    tree: ServiceNameConfigurationKeyMap,
}

// get
impl ServiceValueTree {
    pub fn get(&self) -> &ServiceNameConfigurationKeyMap {
        &self.tree
    }

    pub fn get_by_service_name(
        &self,
        service_name: &String,
    ) -> Option<&ConfigurationKeyExchangeIdMap> {
        Option::from(self.tree.get(service_name)?)
    }

    pub fn get_by_configuration_key(
        &self,
        service_name: &String,
        configuration_key: &String,
    ) -> Option<&ExchangeAccountIdCurrencyCodePairMap> {
        Option::from(
            self.get_by_service_name(service_name)?
                .get(configuration_key)?,
        )
    }

    pub fn get_by_exchange_account_id(
        &self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
    ) -> Option<&CurrencyPairCurrencyCodeMap> {
        Option::from(
            self.get_by_configuration_key(service_name, configuration_key)?
                .get(exchange_account_id)?,
        )
    }

    pub fn get_by_currency_pair(
        &self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Option<&CurrencyCodeValueMap> {
        Option::from(
            self.get_by_exchange_account_id(service_name, configuration_key, exchange_account_id)?
                .get(currency_pair)?,
        )
    }

    pub fn get_by_currency_code(
        &self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
    ) -> Option<Decimal> {
        Option::from(
            self.get_by_currency_pair(
                service_name,
                configuration_key,
                exchange_account_id,
                currency_pair,
            )?
            .get(currency_code)?
            .clone(),
        )
    }
}

impl ServiceValueTree {
    pub fn get_as_balances(&self) -> HashMap<BalanceRequest, Decimal> {
        self.tree
            .iter()
            .map(move |(service_name, service_configurations_keys)| {
                service_configurations_keys
                    .iter()
                    .map(move |(service_configuration_key, exchange_accounts_ids)| {
                        exchange_accounts_ids
                            .iter()
                            .map(move |(exchange_account_id, currencies_pairs)| {
                                currencies_pairs
                                    .iter()
                                    .map(move |(currency_pair, currencies_codes)| {
                                        currencies_codes
                                            .iter()
                                            .map(move |(currency_code, value)| {
                                                (
                                                    BalanceRequest::new(
                                                        Arc::from(ConfigurationDescriptor::new(
                                                            service_name.clone(),
                                                            service_configuration_key.clone(),
                                                        )),
                                                        exchange_account_id.clone(),
                                                        currency_pair.clone(),
                                                        currency_code.clone(),
                                                    ),
                                                    value.clone(),
                                                )
                                            })
                                            .collect_vec()
                                    })
                                    .flatten()
                                    .collect_vec()
                            })
                            .flatten()
                            .collect_vec()
                    })
                    .flatten()
                    .collect_vec()
            })
            .flatten()
            .into_iter()
            .collect()
    }
}
