use std::collections::HashMap;

use crate::balance::manager::balance_request::BalanceRequest;
use crate::exchanges::common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId};
use crate::service_configuration::configuration_descriptor::{
    ConfigurationDescriptor, ServiceConfigurationKey, ServiceName,
};

use mmb_utils::hashmap;
use rust_decimal_macros::dec;

pub(crate) type ConfigurationKeyByServiceName =
    HashMap<ServiceName, ExchangeAccountIdByConfigurationKey>;
pub(crate) type ExchangeAccountIdByConfigurationKey =
    HashMap<ServiceConfigurationKey, CurrencyPairByExchangeAccountId>;
pub(crate) type CurrencyPairByExchangeAccountId =
    HashMap<ExchangeAccountId, CurrencyPairByCurrencyPair>;
pub(crate) type CurrencyPairByCurrencyPair = HashMap<CurrencyPair, ValueByCurrencyCode>;
pub(crate) type ValueByCurrencyCode = HashMap<CurrencyCode, Amount>;

/// A tree that contain balance amounts distributed by
/// ServiceNames -> ConfigurationKeys -> ExchangeAccountIds -> CurrencyPairs -> CurrencyCodes.
///     NOTE: there is storing all balances by ServiceNames(strategy name),
///     that will contain several configuration keys for strategies, next layer is one or more accounts for
///     selected ServiceName and here stored CurrencyCodes by CurrencyPairs and amount for every currency code.
#[derive(Debug, Default, Clone)]
pub struct ServiceValueTree {
    tree: ConfigurationKeyByServiceName,
}
impl ServiceValueTree {
    #[cfg(test)]
    fn get(&self) -> &ConfigurationKeyByServiceName {
        &self.tree
    }

    fn get_mut_by_service_name(
        &mut self,
        service_name: ServiceName,
    ) -> Option<&mut ExchangeAccountIdByConfigurationKey> {
        self.tree.get_mut(&service_name)
    }

    fn get_mut_by_configuration_key(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
    ) -> Option<&mut CurrencyPairByExchangeAccountId> {
        self.get_mut_by_service_name(service_name)?
            .get_mut(&configuration_key)
    }

    fn get_mut_by_exchange_account_id(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
    ) -> Option<&mut CurrencyPairByCurrencyPair> {
        self.get_mut_by_configuration_key(service_name, configuration_key)?
            .get_mut(&exchange_account_id)
    }

    fn get_mut_by_currency_pair(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
    ) -> Option<&mut ValueByCurrencyCode> {
        self.get_mut_by_exchange_account_id(service_name, configuration_key, exchange_account_id)?
            .get_mut(&currency_pair)
    }

    pub fn get_by_balance_request(&self, balance_request: &BalanceRequest) -> Option<Amount> {
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

    pub fn set(&mut self, tree: ConfigurationKeyByServiceName) {
        self.tree = tree;
    }

    pub fn set_by_service_name(
        &mut self,
        service_name: ServiceName,
        value: ExchangeAccountIdByConfigurationKey,
    ) {
        self.tree.insert(service_name, value);
    }

    pub fn set_by_configuration_key(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
        value: CurrencyPairByExchangeAccountId,
    ) {
        if let Some(sub_tree) = self.get_mut_by_service_name(service_name) {
            sub_tree.insert(configuration_key, value);
        } else {
            self.set_by_service_name(service_name, hashmap![configuration_key => value]);
        }
    }

    pub fn set_by_exchange_account_id(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
        value: CurrencyPairByCurrencyPair,
    ) {
        if let Some(sub_tree) = self.get_mut_by_configuration_key(service_name, configuration_key) {
            sub_tree.insert(exchange_account_id, value);
        } else {
            self.set_by_configuration_key(
                service_name,
                configuration_key,
                hashmap![exchange_account_id => value],
            );
        }
    }

    pub fn set_by_currency_pair(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        value: ValueByCurrencyCode,
    ) {
        if let Some(sub_tree) = self.get_mut_by_exchange_account_id(
            service_name,
            configuration_key,
            exchange_account_id,
        ) {
            sub_tree.insert(currency_pair, value);
        } else {
            self.set_by_exchange_account_id(
                service_name,
                configuration_key,
                exchange_account_id,
                hashmap![currency_pair => value],
            );
        }
    }

    pub fn set_by_currency_code(
        &mut self,
        service_name: ServiceName,
        configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        currency_code: CurrencyCode,
        value: Amount,
    ) {
        if let Some(sub_tree) = self.get_mut_by_currency_pair(
            service_name,
            configuration_key,
            exchange_account_id,
            currency_pair,
        ) {
            sub_tree.insert(currency_code, value);
        } else {
            self.set_by_currency_pair(
                service_name,
                configuration_key,
                exchange_account_id,
                currency_pair,
                hashmap![currency_code => value],
            );
        }
    }

    pub fn set_by_balance_request(&mut self, balance_request: &BalanceRequest, value: Amount) {
        self.set_by_currency_code(
            balance_request.configuration_descriptor.service_name,
            balance_request
                .configuration_descriptor
                .service_configuration_key,
            balance_request.exchange_account_id,
            balance_request.currency_pair,
            balance_request.currency_code,
            value,
        );
    }

    pub fn get_as_balances(&self) -> HashMap<BalanceRequest, Amount> {
        self.tree
            .iter()
            .flat_map(move |(service_name, service_configuration_keys)| {
                service_configuration_keys.iter().flat_map(
                    move |(service_configuration_key, exchange_accounts_ids)| {
                        exchange_accounts_ids.iter().flat_map(
                            move |(exchange_account_id, currency_pairs)| {
                                currency_pairs.iter().flat_map(
                                    move |(currency_pair, currency_codes)| {
                                        currency_codes.iter().map(move |(currency_code, value)| {
                                            (
                                                BalanceRequest::new(
                                                    ConfigurationDescriptor::new(
                                                        *service_name,
                                                        *service_configuration_key,
                                                    ),
                                                    *exchange_account_id,
                                                    *currency_pair,
                                                    *currency_code,
                                                ),
                                                *value,
                                            )
                                        })
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

    pub fn add_by_request(&mut self, request: &BalanceRequest, value: Amount) {
        self.set_by_balance_request(
            request,
            self.get_by_balance_request(request).unwrap_or(dec!(0)) + value,
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};

    use mmb_utils::{hashmap, logger::init_logger_file_named};
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
        .map(|x| ExchangeAccountId::new(x.0, x.1))
        .to_vec()
    }
    fn get_configuration_keys() -> Vec<ServiceConfigurationKey> {
        ["cc0", "cc1", "cc2", "cc3", "cc4"]
            .map(|x| x.into())
            .to_vec()
    }

    fn get_service_names() -> Vec<ServiceName> {
        ["name0", "name1", "name2", "name3", "name4"]
            .map(|x| x.into())
            .to_vec()
    }

    fn get_test_data() -> (ServiceValueTree, HashMap<BalanceRequest, Amount>) {
        let mut service_value_tree = ServiceValueTree::default();
        let mut balances = HashMap::new();
        for service_name in get_service_names() {
            for service_configuration_key in get_configuration_keys() {
                for exchange_account_id in get_exchange_account_ids() {
                    for currency_pair in get_currency_pairs() {
                        for currency_code in get_currency_codes() {
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
                                        service_name,
                                        service_configuration_key,
                                    ),
                                    exchange_account_id,
                                    currency_pair,
                                    currency_code,
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
        init_logger_file_named("log.txt");
        let test_data = get_test_data();
        assert_eq!(test_data.0.get_as_balances(), test_data.1);
    }

    #[test]
    pub fn set() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        let new_service_configuration_key = "new_name".into();
        let new_exchange_account_id = ExchangeAccountId::new("new_acc", 0);
        let new_currency_pair = CurrencyPair::from_codes("new_code_b".into(), "new_code_q".into());
        let new_currency_code = CurrencyCode::new("new_code");
        let new_value = dec!(1);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![currency_pair => hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            value,
            Some("original trees are same"),
        );

        service_value_tree.set_by_currency_code(
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            new_value,
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            new_value,
            Some("trees remain identical after changing 'value'"),
        );

        let new_map = hashmap![new_currency_code => new_value];
        service_value_tree.set_by_currency_pair(
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            new_map.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            new_currency_code,
            new_value,
            Some("trees remain identical after changing 'currency_code'"),
        );

        let new_map = hashmap![new_currency_pair => new_map];
        service_value_tree.set_by_exchange_account_id(
            service_name,
            service_configuration_key,
            exchange_account_id,
            new_map.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            new_currency_pair,
            new_currency_code,
            new_value,
            Some("trees remain identical after changing 'currency_pair'"),
        );

        let new_map = hashmap![new_exchange_account_id => new_map];
        service_value_tree.set_by_configuration_key(
            service_name,
            service_configuration_key,
            new_map.clone(),
        );
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            new_exchange_account_id,
            new_currency_pair,
            new_currency_code,
            new_value,
            Some("trees remain identical after changing 'exchange_account_id'"),
        );

        let new_map = hashmap![new_service_configuration_key => new_map];
        service_value_tree.set_by_service_name(service_name, new_map);
        assert_tree_item_eq_with_message(
            service_value_tree.get(),
            service_name,
            new_service_configuration_key,
            new_exchange_account_id,
            new_currency_pair,
            new_currency_code,
            new_value,
            Some("trees remain identical after changing 'service_configuration_key'"),
        );
    }

    #[test]
    pub fn add() {
        init_logger_file_named("log.txt");
        let mut test_data = get_test_data();

        let new_service_name = "new_service_name".into();
        let new_service_configuration_key = "new_service_configuration_key".into();
        let new_exchange_account_id = ExchangeAccountId::new("new_exchange_account_id", 0);
        let new_currency_pair = CurrencyPair::from_codes(
            "new_currency_pair_b_0".into(),
            "new_currency_pair_q_0".into(),
        );
        let new_currency_code = CurrencyCode::new("0");
        let new_value = dec!(0);

        let mut new_tree = ServiceValueTree::default();
        new_tree.set_by_currency_code(
            new_service_name,
            new_service_configuration_key,
            new_exchange_account_id,
            new_currency_pair,
            new_currency_code,
            new_value,
        );

        test_data.1.insert(
            BalanceRequest::new(
                ConfigurationDescriptor::new(new_service_name, new_service_configuration_key),
                new_exchange_account_id,
                new_currency_pair,
                new_currency_code,
            ),
            new_value,
        );

        new_tree.add(&test_data.0);
        assert_eq!(new_tree.get_as_balances(), test_data.1);
    }

    #[test]
    pub fn compare() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_value() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            dec!(1),
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_currency_code() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            CurrencyCode::new("1"),
            value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_currency_pair() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            exchange_account_id,
            CurrencyPair::from_codes("new_b_0".into(), "q_0".into()),
            currency_code,
            value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_exchange_account_id() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            service_name,
            service_configuration_key,
            ExchangeAccountId::new("acc_0", 1),
            currency_pair,
            currency_code,
            value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_service_configuration_key() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                    service_configuration_key =>
                    hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            service_name,
            "new_name".into(),
            exchange_account_id,
            currency_pair,
            currency_code,
            value,
        );
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`")]
    pub fn compare_failed_by_service_name() {
        init_logger_file_named("log.txt");
        let mut service_value_tree = ServiceValueTree::default();

        let service_name = "name".into();
        let service_configuration_key = "name".into();
        let exchange_account_id = ExchangeAccountId::new("acc_0", 0);
        let currency_pair = CurrencyPair::from_codes("b_0".into(), "q_0".into());
        let currency_code = CurrencyCode::new("0");
        let value = dec!(0);

        service_value_tree.set_by_service_name(
            service_name,
            hashmap![
                service_configuration_key =>
                hashmap![
                    exchange_account_id =>
                    hashmap![
                        currency_pair =>
                        hashmap![currency_code => value]
                    ]
                ]
            ],
        );

        assert_tree_item_eq(
            service_value_tree.get(),
            "new_name".into(),
            service_configuration_key,
            exchange_account_id,
            currency_pair,
            currency_code,
            value,
        );
    }

    fn assert_tree_item_eq(
        tree: &ConfigurationKeyByServiceName,
        service_name: ServiceName,
        service_configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        currency_code: CurrencyCode,
        value: Amount,
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

    #[allow(clippy::too_many_arguments)]
    fn assert_tree_item_eq_with_message(
        tree: &ConfigurationKeyByServiceName,
        service_name: ServiceName,
        service_configuration_key: ServiceConfigurationKey,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        currency_code: CurrencyCode,
        value: Amount,
        err_message: Option<&str>,
    ) {
        assert_eq!(
            tree.clone(),
            hashmap![
                service_name =>
                hashmap![
                    service_configuration_key =>
                    hashmap![
                        exchange_account_id =>
                        hashmap![
                            currency_pair => hashmap![currency_code => value]
                        ]
                    ]
                ]
            ],
            "{}",
            err_message.unwrap_or("trees not equal")
        );
    }
}
