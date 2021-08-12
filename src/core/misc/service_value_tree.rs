use std::collections::HashMap;

use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};

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

    pub fn get_by_exchange_id(
        &self,
        service_name: &String,
        configuration_key: &String,
        exchange_id: &ExchangeAccountId,
    ) -> Option<&CurrencyPairCurrencyCodeMap> {
        Option::from(
            self.get_by_configuration_key(service_name, configuration_key)?
                .get(exchange_id)?,
        )
    }

    pub fn get_by_currency_pair(
        &self,
        service_name: &String,
        configuration_key: &String,
        exchange_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Option<&CurrencyCodeValueMap> {
        Option::from(
            self.get_by_exchange_id(service_name, configuration_key, exchange_id)?
                .get(currency_pair)?,
        )
    }

    pub fn get_by_currency_code(
        &self,
        service_name: &String,
        configuration_key: &String,
        exchange_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
    ) -> Option<Decimal> {
        Option::from(
            self.get_by_currency_pair(service_name, configuration_key, exchange_id, currency_pair)?
                .get(currency_code)?
                .clone(),
        )
    }
}
