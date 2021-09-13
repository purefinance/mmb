use std::collections::HashMap;
use std::sync::Arc;

use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};

use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::hashmap;

use itertools::Itertools;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub(crate) type ServiceNameConfigurationKeyMap =
    HashMap<String, ConfigurationKeyExchangeAccountIdMap>;
pub(crate) type ConfigurationKeyExchangeAccountIdMap =
    HashMap<String, ExchangeAccountIdCurrencyCodePairMap>;
pub(crate) type ExchangeAccountIdCurrencyCodePairMap =
    HashMap<ExchangeAccountId, CurrencyPairCurrencyCodeMap>;
pub(crate) type CurrencyPairCurrencyCodeMap = HashMap<CurrencyPair, CurrencyCodeValueMap>;
pub(crate) type CurrencyCodeValueMap = HashMap<CurrencyCode, Decimal>;

#[derive(Debug, Clone)]
pub struct ServiceValueTree {
    tree: ServiceNameConfigurationKeyMap,
}
impl ServiceValueTree {
    fn get(&self) -> &ServiceNameConfigurationKeyMap {
        &self.tree
    }

    fn get_mut(&mut self) -> &mut ServiceNameConfigurationKeyMap {
        &mut self.tree
    }

    fn get_mut_by_service_name(
        &mut self,
        service_name: &String,
    ) -> Option<&mut ConfigurationKeyExchangeAccountIdMap> {
        self.tree.get_mut(service_name)
    }

    fn get_mut_by_configuration_key(
        &mut self,
        service_name: &String,
        configuration_key: &String,
    ) -> Option<&mut ExchangeAccountIdCurrencyCodePairMap> {
        self.get_mut_by_service_name(service_name)?
            .get_mut(configuration_key)
    }

    fn get_mut_by_exchange_account_id(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
    ) -> Option<&mut CurrencyPairCurrencyCodeMap> {
        self.get_mut_by_configuration_key(service_name, configuration_key)?
            .get_mut(exchange_account_id)
    }

    fn get_mut_by_currency_pair(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Option<&mut CurrencyCodeValueMap> {
        self.get_mut_by_exchange_account_id(service_name, configuration_key, exchange_account_id)?
            .get_mut(currency_pair)
    }

    fn get_mut_by_currency_code(
        &mut self,
        service_name: &String,
        configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
    ) -> Option<&mut Decimal> {
        self.get_mut_by_currency_pair(
            service_name,
            configuration_key,
            exchange_account_id,
            currency_pair,
        )?
        .get_mut(currency_code)
    }

    pub fn get_by_balance_request(&self, balance_request: &BalanceRequest) -> Option<Decimal> {
        self.tree
            .get(&balance_request.configuration_descriptor.service_name)?
            .get(
                &balance_request
                    .configuration_descriptor
                    .service_configuration_key,
            )?
            .get(&balance_request.exchange_account_id)?
            .get(&balance_request.currency_pair)?
            .get(&balance_request.currency_code)
            .cloned()
    }

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
            self.set_by_service_name(service_name, hashmap![configuration_key.clone() => value]);
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
                hashmap![exchange_account_id.clone() => value],
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
                hashmap![currency_pair.clone() => value],
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
                hashmap![currency_code.clone() => value],
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

    pub fn new() -> Self {
        Self {
            tree: HashMap::new(),
        }
    }
    pub fn get_as_balances(&self) -> HashMap<BalanceRequest, Decimal> {
        self.tree
            .iter()
            .flat_map(move |(service_name, service_configuration_keys)| {
                service_configuration_keys.iter().flat_map(
                    move |(service_configuration_key, exchange_accounts_ids)| {
                        exchange_accounts_ids.iter().flat_map(
                            move |(exchange_account_id, currency_pairs)| {
                                currency_pairs.iter().flat_map(
                                    move |(currency_pair, currency_codes)| {
                                        currency_codes
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
                                    },
                                )
                            },
                        )
                    },
                )
            })
            .collect()
    }

    pub fn add(&mut self, input: &ServiceValueTree) {
        for (request, value) in input.get_as_balances() {
            self.add_by_request(&request, value);
        }
    }

    pub fn add_by_request(&mut self, request: &BalanceRequest, value: Decimal) {
        self.set_by_balance_request(
            request,
            self.get_by_balance_request(request).unwrap_or(dec!(0)) + value,
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};
    use crate::core::logger::init_logger;
    use crate::hashmap;

    use rust_decimal_macros::dec;

    use std::collections::HashMap;

    fn get_currency_codes() -> Vec<CurrencyCode> {
        ["0", "1", "2", "3", "4"].map(|x| x.into()).to_vec()
    }

    fn get_currency_pairs() -> Vec<CurrencyPair> {
        [
            ("b_0", "q_0"),
            ("b_1", "q_1"),
            ("b_2", "q_2"),
            ("b_3", "q_3"),
            ("b_4", "q_4"),
        ]
        .map(|x| CurrencyPair::from_codes(x.0.into(), x.1.into()))
        .to_vec()
    }

    fn get_exchange_account_ids() -> Vec<ExchangeAccountId> {
        [
            ("acc_0", 0),
            ("acc_1", 1),
            ("acc_2", 2),
            ("acc_3", 3),
            ("acc_4", 4),
        ]
        .map(|x| ExchangeAccountId::new(x.0.into(), x.1))
        .to_vec()
    }
    fn get_configuration_keys() -> Vec<String> {
        ["cc0", "cc1", "cc2", "cc3", "cc4"]
            .map(|x| x.into())
            .to_vec()
    }

    fn get_service_names() -> Vec<String> {
        ["name0", "name1", "name2", "name3", "name4"]
            .map(|x| x.into())
            .to_vec()
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
                                    Arc::from(ConfigurationDescriptor::new(
                                        service_name.clone(),
                                        service_configuration_key.clone(),
                                    )),
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
    pub fn get_as_balances() {
        init_logger();
        let test_data = get_test_data();
        assert_eq!(test_data.0.get_as_balances(), test_data.1);
    }

    #[test]
    pub fn set() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        let new_service_configuration_key = "new_name".to_string();
        let new_exchange_account_id = ExchangeAccountId::new("new_acc".into(), 0);
        let new_currency_pair = CurrencyPair::from_codes("new_code_b".into(), "new_code_q".into());
        let new_currency_code = CurrencyCode::new("new_code".into());
        let new_value = dec!(1);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &value,
            Some("original trees are same"),
        );

        service_value_tree.set_by_currency_code(
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            new_value.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &new_value,
            Some("trees remain the same after changing 'value'"),
        );

        let new_map = hashmap![new_currency_code.clone() => new_value.clone()];
        service_value_tree.set_by_currency_pair(
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            new_map.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &new_currency_code,
            &new_value,
            Some("trees remain the same after changing 'currency_code'"),
        );

        let new_map = hashmap![new_currency_pair.clone() => new_map.clone()];
        service_value_tree.set_by_exchange_account_id(
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            new_map.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            &new_value,
            Some("trees remain the same after changing 'currency_pair'"),
        );

        let new_map = hashmap![new_exchange_account_id.clone() => new_map.clone()];
        service_value_tree.set_by_configuration_key(
            &service_name,
            &service_configuration_key,
            new_map.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &new_exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            &new_value,
            Some("trees remain the same after changing 'exchange_account_id'"),
        );

        let new_map = hashmap![new_service_configuration_key.clone() => new_map.clone()];
        service_value_tree.set_by_service_name(&service_name, new_map.clone());
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            &service_name,
            &new_service_configuration_key,
            &new_exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            &new_value,
            Some("trees remain the same after changing 'service_configuration_key'"),
        );
    }

    #[test]
    pub fn add() {
        init_logger();
        let mut test_data = get_test_data();
        assert_eq!(test_data.0.get_as_balances(), test_data.1);

        let new_service_name = "new_service_name".to_string();
        let new_service_configuration_key = "new_service_configuration_key".to_string();
        let new_exchange_account_id = ExchangeAccountId::new("new_exchange_account_id".into(), 0);
        let new_currency_pair = CurrencyPair::from_codes(
            "new_currency_pair_b_0".into(),
            "new_currency_pair_q_0".into(),
        );
        let new_currency_code = CurrencyCode::new("0".into());
        let new_value = dec!(0);

        let mut new_tree = ServiceValueTree::new();
        new_tree.set_by_currency_code(
            &new_service_name,
            &new_service_configuration_key,
            &new_exchange_account_id,
            &new_currency_pair,
            &new_currency_code,
            new_value,
        );

        test_data.1.insert(
            BalanceRequest::new(
                Arc::from(ConfigurationDescriptor::new(
                    new_service_name.clone(),
                    new_service_configuration_key.clone(),
                )),
                new_exchange_account_id.clone(),
                new_currency_pair.clone(),
                new_currency_code.clone(),
            ),
            new_value,
        );

        new_tree.add(&test_data.0);
        assert_eq!(new_tree.get_as_balances(), test_data.1);
    }

    #[test]
    pub fn compare() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_value() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &dec!(1),
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_currency_code() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &CurrencyCode::new("1".into()),
            &value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_currency_pair() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &exchange_account_id,
            &CurrencyPair::from_codes("m_b_0".into(), "q_0".into()),
            &currency_code,
            &value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_exchange_account_id() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &service_name,
            &service_configuration_key,
            &ExchangeAccountId::new("acc_0".into(), 1),
            &currency_pair,
            &currency_code,
            &value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_service_configuration_key() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &service_name,
            &"new_name".to_string(),
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_service_name() {
        init_logger();
        let mut service_value_tree = ServiceValueTree::new();

        let service_name = "name".to_string();
        let service_configuration_key = "name".to_string();
        let exchange_account_id = ExchangeAccountId::new("acc_0".into(), 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0".into());
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            &service_name,
            hashmap![
                service_configuration_key.clone() =>
                hashmap![
                    exchange_account_id.clone() =>
                    hashmap![
                        currency_pair.clone() =>
                        hashmap![currency_code.clone() => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            &"new_name".to_string(),
            &service_configuration_key,
            &exchange_account_id,
            &currency_pair,
            &currency_code,
            &value,
        );
    }

    fn assert_tree_item_eq(
        tree: &ServiceNameConfigurationKeyMap,
        service_name: &String,
        service_configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
        value: &Decimal,
    ) {
        assert_tree_item_eq_with_message(
            tree,
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            value,
            None,
        );
    }

    fn assert_tree_item_eq_with_message(
        tree: &ServiceNameConfigurationKeyMap,
        service_name: &String,
        service_configuration_key: &String,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        currency_code: &CurrencyCode,
        value: &Decimal,
        err_message: Option<&str>,
    ) {
        assert_eq!(
            tree.clone(),
            hashmap![
                service_name.clone() =>
                hashmap![
                    service_configuration_key.clone() =>
                    hashmap![
                        exchange_account_id.clone() =>
                        hashmap![
                            currency_pair.clone() =>
                            hashmap![currency_code.clone() => value.clone()]
                        ]
                    ]
                ]
            ],
            "{}",
            err_message.unwrap_or("trees not equal")
        );
    }
}
