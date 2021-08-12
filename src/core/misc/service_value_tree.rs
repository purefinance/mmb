use std::collections::HashMap;

use rust_decimal::Decimal;

pub type ServiceNameConfigurationKeyMap = HashMap<String, ConfigurationKeyExchangeIdMap>;
pub type ConfigurationKeyExchangeIdMap = HashMap<String, ExchangeIdCurrencyCodePairMap>;
pub type ExchangeIdCurrencyCodePairMap = HashMap<String, CurrencyCodePairCurrencyCodeMap>;
pub type CurrencyCodePairCurrencyCodeMap = HashMap<String, CurrencyCodeValueMap>;
pub type CurrencyCodeValueMap = HashMap<String, Decimal>;

pub(crate) struct ServiceValueTree {
    tree: ServiceNameConfigurationKeyMap,
}
