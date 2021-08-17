use std::collections::HashMap;
use std::sync::Arc;

use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};
use crate::core::misc::make_hash_map::make_hash_map;
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

use itertools::Itertools;
use rust_decimal::Decimal;

pub(crate) type ServiceNameConfigurationKeyMap =
    HashMap<String, ConfigurationKeyExchangeAccountIdMap>;
pub(crate) type ConfigurationKeyExchangeAccountIdMap =
    HashMap<String, ExchangeAccountIdCurrencyCodePairMap>;
pub(crate) type ExchangeAccountIdCurrencyCodePairMap =
    HashMap<ExchangeAccountId, CurrencyPairCurrencyCodeMap>;
pub(crate) type CurrencyPairCurrencyCodeMap = HashMap<CurrencyPair, CurrencyCodeValueMap>;
pub(crate) type CurrencyCodeValueMap = HashMap<CurrencyCode, Decimal>;

#[derive(Debug)]
pub(crate) struct ServiceValueTree {
    tree: ServiceNameConfigurationKeyMap,
}
// get
impl ServiceValueTree {
    fn get(&mut self) -> &mut ServiceNameConfigurationKeyMap {
        &mut self.tree
    }

    fn get_mut_by_service_name(
        &mut self,
        service_name: &String,
    ) -> Option<&mut ConfigurationKeyExchangeAccountIdMap> {
        Option::from(self.tree.get_mut(service_name)?)
    }

    fn get_mut_by_configuration_key(
        &mut self,
        service_name: &String,
        configuration_key: &String,
    ) -> Option<&mut ExchangeAccountIdCurrencyCodePairMap> {
        Option::from(
            self.get_mut_by_service_name(service_name)?
                .get_mut(configuration_key)?,
        )
    }

    fn get_mut_by_exchange_account_id(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
    ) -> Option<&mut CurrencyPairCurrencyCodeMap> {
        Option::from(
            self.get_mut_by_configuration_key(service_name, configuration_key)?
                .get_mut(exchange_account_id)?,
        )
    }

    fn get_mut_by_currency_pair(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Option<&mut CurrencyCodeValueMap> {
        Option::from(
            self.get_mut_by_exchange_account_id(
                service_name,
                configuration_key,
                exchange_account_id,
            )?
            .get_mut(currency_pair)?,
        )
    }

    fn get_mut_by_currency_code(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
    ) -> Option<&mut Decimal> {
        Option::from(
            self.get_mut_by_currency_pair(
                service_name,
                configuration_key,
                exchange_account_id,
                currency_pair,
            )?
            .get_mut(currency_code)?,
        )
    }

    pub fn get_by_balance_request(&self, balance_request: &BalanceRequest) -> Option<Decimal> {
        Some(
            self.tree
                .get(&balance_request.configuration_descriptor.service_name)?
                .get(
                    &balance_request
                        .configuration_descriptor
                        .service_configuration_key,
                )?
                .get(&balance_request.exchange_account_id)?
                .get(&balance_request.currency_pair)?
                .get(&balance_request.currency_code)?
                .clone(),
        )
    }
}

// set
impl ServiceValueTree {
    pub fn set(&mut self, tree: ServiceNameConfigurationKeyMap) {
        self.tree = tree;
    }

    pub fn set_by_service_name(
        &mut self,
        service_name: &String,
        value: ConfigurationKeyExchangeAccountIdMap,
    ) {
        self.tree.insert(service_name.clone(), value);
    }

    pub fn set_by_configuration_key(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        value: ExchangeAccountIdCurrencyCodePairMap,
    ) {
        if let Some(sub_tree) = self.get_mut_by_service_name(service_name) {
            sub_tree.insert(configuration_key.clone(), value);
        } else {
            self.set_by_service_name(
                service_name,
                make_hash_map(configuration_key.clone(), value),
            );
        }
    }

    pub fn set_by_exchange_account_id(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        value: CurrencyPairCurrencyCodeMap,
    ) {
        if let Some(sub_tree) = self.get_mut_by_configuration_key(service_name, configuration_key) {
            sub_tree.insert(exchange_account_id.clone(), value);
        } else {
            self.set_by_configuration_key(
                service_name,
                configuration_key,
                make_hash_map(exchange_account_id.clone(), value),
            );
        }
    }

    pub fn set_by_currency_pair(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        value: CurrencyCodeValueMap,
    ) {
        if let Some(sub_tree) = self.get_mut_by_exchange_account_id(
            service_name,
            configuration_key,
            exchange_account_id,
        ) {
            sub_tree.insert(currency_pair.clone(), value);
        } else {
            self.set_by_exchange_account_id(
                service_name,
                configuration_key,
                exchange_account_id,
                make_hash_map(currency_pair.clone(), value),
            );
        }
    }

    pub fn set_by_currency_code(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
        value: Decimal,
    ) {
        if let Some(sub_tree) = self.get_mut_by_currency_pair(
            service_name,
            configuration_key,
            exchange_account_id,
            currency_pair,
        ) {
            sub_tree.insert(currency_code.clone(), value);
        } else {
            self.set_by_currency_pair(
                service_name,
                configuration_key,
                exchange_account_id,
                currency_pair,
                make_hash_map(currency_code.clone(), value),
            );
        }
    }

    pub fn set_by_balance_request(&mut self, balance_request: &BalanceRequest, value: Decimal) {
        self.set_by_currency_code(
            &balance_request.configuration_descriptor.service_name,
            &balance_request
                .configuration_descriptor
                .service_configuration_key,
            &balance_request.exchange_account_id,
            &balance_request.currency_pair,
            &balance_request.currency_code,
            value,
        );
    }
}

impl ServiceValueTree {
    pub fn new() -> Self {
        Self {
            tree: HashMap::new(),
        }
    }
    pub fn get_as_balances(&self) -> HashMap<BalanceRequest, Decimal> {
        self.tree
            .iter()
            .map(move |(service_name, service_configuration_keys)| {
                service_configuration_keys
                    .iter()
                    .map(move |(service_configuration_key, exchange_accounts_ids)| {
                        exchange_accounts_ids
                            .iter()
                            .map(move |(exchange_account_id, currency_pairs)| {
                                currency_pairs
                                    .iter()
                                    .map(move |(currency_pair, currency_codes)| {
                                        currency_codes
                                            .iter()
                                            .map(move |(currency_code, value)| {
                                                (
                                                    BalanceRequest::new(
                                                        ConfigurationDescriptor::new(
                                                            service_name.clone(),
                                                            service_configuration_key.clone(),
                                                        ),
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
#[cfg(test)]
mod test {
    use super::*;

    use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};
    use crate::core::logger::init_logger;
    use crate::core::misc::make_hash_map::make_hash_map;

    use rust_decimal_macros::dec;

    use std::collections::HashMap;

    fn get_currency_codes() -> Vec<CurrencyCode> {
        vec![
            CurrencyCode::new("0".into()),
            CurrencyCode::new("1".into()),
            CurrencyCode::new("2".into()),
            CurrencyCode::new("3".into()),
            CurrencyCode::new("4".into()),
        ]
    }

    fn get_currency_pairs() -> Vec<CurrencyPair> {
        vec![
            CurrencyPair::from_codes("b_0".into(), "q_0".into()),
            CurrencyPair::from_codes("b_1".into(), "q_1".into()),
            CurrencyPair::from_codes("b_2".into(), "q_2".into()),
            CurrencyPair::from_codes("b_3".into(), "q_3".into()),
            CurrencyPair::from_codes("b_4".into(), "q_4".into()),
        ]
    }

    fn get_exchange_account_ids() -> Vec<ExchangeAccountId> {
        vec![
            ExchangeAccountId::new("acc_0".into(), 0),
            ExchangeAccountId::new("acc_1".into(), 1),
            ExchangeAccountId::new("acc_2".into(), 2),
            ExchangeAccountId::new("acc_3".into(), 3),
            ExchangeAccountId::new("acc_4".into(), 4),
        ]
    }
    fn get_configuration_keys() -> Vec<String> {
        vec![
            String::from("cc0"),
            String::from("cc1"),
            String::from("cc2"),
            String::from("cc3"),
            String::from("cc4"),
        ]
    }

    fn get_service_names() -> Vec<String> {
        vec![
            String::from("name0"),
            String::from("name1"),
            String::from("name2"),
            String::from("name3"),
            String::from("name4"),
        ]
    }

    fn get_test_data() -> (ServiceValueTree, HashMap<BalanceRequest, Decimal>) {
        let mut service_value_tree = ServiceValueTree::new();
        let mut balances = HashMap::new();
        for service_name in &get_service_names() {
            for service_configuration_key in &get_configuration_keys() {
                for exchange_account_id in &get_exchange_account_ids() {
                    for currency_pair in &get_currency_pairs() {
                        for currency_code in &get_currency_codes() {
                            service_value_tree.set_by_currency_code(
                                service_name,
                                service_configuration_key,
                                exchange_account_id,
                                currency_pair,
                                currency_code,
                                dec!(1),
                            );
                            balances.insert(
                                BalanceRequest::new(
                                    ConfigurationDescriptor::new(
                                        service_name.clone(),
                                        service_configuration_key.clone(),
                                    ),
                                    exchange_account_id.clone(),
                                    currency_pair.clone(),
                                    currency_code.clone(),
                                ),
                                dec!(1),
                            );
                        }
                    }
                }
            }
        }
        (service_value_tree, balances)
    }

    #[test]
    pub fn get_as_balances_test() {
        init_logger();
        let test_data = get_test_data();
        assert_eq!(test_data.0.get_as_balances(), test_data.1);
    }

    #[test]
    pub fn set_test() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = String::from("name");
        let service_configuration_key = String::from("name");
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        let new_service_configuration_key = String::from("m_name");
        let new_exchange_account_id = ExchangeAccountId::new("m_acc_0".into(), 0);
        let new_currency_pair = CurrencyPair::from_codes("m_b_0".into(), "m_q_0".into());
        let new_currency_code = CurrencyCode::new("m_0".into());
        let new_value = dec!(1);

        service_value_tree.set_by_service_name(
            &service_name,
            make_hash_map(
                service_configuration_key.clone(),
                make_hash_map(
                    exchange_account_id.clone(),
                    make_hash_map(
                        currency_pair.clone(),
                        make_hash_map(currency_code.clone(), value),
                    ),
                ),
            ),
        );

        service_value_tree.set_by_currency_code(
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            new_value.clone(),
        );
        compare_trees(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &new_value,
        );
        log::info!("trees remain the same after changing 'value'");

        let new_map = make_hash_map(new_currency_code.clone(), new_value.clone());
        service_value_tree.set_by_currency_pair(
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            new_map.clone(),
        );
        compare_trees(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &new_currency_code,
            &new_value,
        );
        log::info!("trees remain the same after changing 'currency_code'");

        let new_map = make_hash_map(new_currency_pair.clone(), new_map.clone());
        service_value_tree.set_by_exchange_account_id(
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            new_map.clone(),
        );
        compare_trees(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            &new_value,
        );
        log::info!("trees remain the same after changing 'currency_pair'");

        let new_map = make_hash_map(new_exchange_account_id.clone(), new_map.clone());
        service_value_tree.set_by_configuration_key(
            &service_name,
            &service_configuration_key,
            new_map.clone(),
        );
        compare_trees(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &new_exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            &new_value,
        );
        log::info!("trees remain the same after changing 'exchange_account_id'");

        let new_map = make_hash_map(new_service_configuration_key.clone(), new_map.clone());
        service_value_tree.set_by_service_name(&service_name, new_map.clone());
        compare_trees(
            service_value_tree.get(),
            &service_name,
            &new_service_configuration_key,
            &new_exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            &new_value,
        );
        log::info!("trees remain the same after changing 'service_configuration_key'");
    }

    fn compare_trees(
        tree: &ServiceNameConfigurationKeyMap,
        service_name: &String,
        service_configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
        value: &Decimal,
    ) {
        assert_eq!(
            tree.clone(),
            make_hash_map(
                service_name.clone(),
                make_hash_map(
                    service_configuration_key.clone(),
                    make_hash_map(
                        exchange_account_id.clone(),
                        make_hash_map(
                            currency_pair.clone(),
                            make_hash_map(currency_code.clone(), value.clone()),
                        ),
                    ),
                )
            )
        );
    }
}
