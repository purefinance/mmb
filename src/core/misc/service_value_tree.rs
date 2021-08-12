use std::collections::HashMap;

use rust_decimal::Decimal;

pub(crate) type ServiceNameConfigurationKeyMap = HashMap<String, ConfigurationKeyExchangeIdMap>;
pub(crate) type ConfigurationKeyExchangeIdMap = HashMap<String, ExchangeIdCurrencyCodePairMap>;
pub(crate) type ExchangeIdCurrencyCodePairMap = HashMap<String, CurrencyCodePairCurrencyCodeMap>;
pub(crate) type CurrencyCodePairCurrencyCodeMap = HashMap<String, CurrencyCodeValueMap>;
pub(crate) type CurrencyCodeValueMap = HashMap<String, Decimal>;

pub(crate) struct ServiceValueTree {
    tree: ServiceNameConfigurationKeyMap,
}
