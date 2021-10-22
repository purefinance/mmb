use std::collections::HashMap;
use std::sync::Arc;

use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::exchanges::common::{Amount, CurrencyCode, ExchangeAccountId, Price};
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::exchanges::general::exchange::Exchange;
use crate::core::explanation::Explanation;
use crate::core::misc::service_value_tree::ServiceValueTree;

use rust_decimal_macros::dec;

type BalanceByExchangeId = HashMap<ExchangeAccountId, HashMap<CurrencyCode, Amount>>;

#[derive(Clone)]
pub(crate) struct VirtualBalanceHolder {
    balance_by_exchange_id: BalanceByExchangeId,
    balance_diff: ServiceValueTree,
}

impl VirtualBalanceHolder {
    pub fn new(exchanges_by_id: HashMap<ExchangeAccountId, Arc<Exchange>>) -> Self {
        let balance_by_exchange_id = exchanges_by_id
            .keys()
            .map(|x| (*x, HashMap::new()))
            .collect();

        Self {
            balance_by_exchange_id,
            balance_diff: ServiceValueTree::new(),
        }
    }

    pub fn update_balances(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        balances_by_currency_code: &HashMap<CurrencyCode, Amount>,
    ) {
        self.balance_by_exchange_id
            .insert(exchange_account_id, balances_by_currency_code.clone());

        log::info!(
            "VirtualBalanceHolder::update_balances {} {:?}",
            exchange_account_id,
            balances_by_currency_code
        );

        let all_diffs = self.balance_diff.get_as_balances();
        for currency_code in balances_by_currency_code.keys() {
            for (balance_request, _) in &all_diffs {
                if balance_request.exchange_account_id == exchange_account_id
                    && balance_request.currency_code == *currency_code
                {
                    self.balance_diff
                        .set_by_balance_request(balance_request, dec!(0));
                    log::info!(
                        "VirtualBalanceHolder::update_balances Reset {} {}",
                        balance_request.exchange_account_id,
                        balance_request.currency_code
                    );
                }
            }
        }
    }

    pub fn add_balance(&mut self, balance_request: &BalanceRequest, balance_to_add: Amount) {
        let current_diff_value = self
            .balance_diff
            .get_by_balance_request(balance_request)
            .unwrap_or(dec!(0));

        let new_value = current_diff_value + balance_to_add;
        self.balance_diff
            .set_by_balance_request(balance_request, new_value);

        log::info!(
            "VirtualBalanceHolder::add_balance {} {} {} {} {} {}",
            balance_request.exchange_account_id,
            balance_request.currency_pair,
            balance_request.currency_code,
            current_diff_value,
            balance_to_add,
            new_value
        );
    }

    pub fn add_balance_by_currency_pair_metadata(
        &mut self,
        request: &BalanceRequest,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        diff_in_amount_currency: Amount,
        price: Price,
    ) {
        if !currency_pair_metadata.is_derivative {
            let diff_in_request_currency = currency_pair_metadata
                .convert_amount_from_amount_currency_code(
                    request.currency_code,
                    diff_in_amount_currency,
                    price,
                );
            self.add_balance(request, diff_in_request_currency);
        } else {
            let balance_currency_code_request = BalanceRequest::new(
                request.configuration_descriptor.clone(),
                request.exchange_account_id,
                request.currency_pair,
                currency_pair_metadata
                    .balance_currency_code
                    .expect("currency_pair_metadata.balance_currency_code should be non None"),
            );
            let diff_in_balance_currency_code = currency_pair_metadata
                .convert_amount_from_amount_currency_code(
                    balance_currency_code_request.currency_code,
                    diff_in_amount_currency,
                    price,
                );
            self.add_balance(
                &balance_currency_code_request,
                diff_in_balance_currency_code,
            );
        }
    }

    pub fn get_virtual_balance(
        &self,
        balance_request: &BalanceRequest,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        price: Option<Price>,
        explanation: &mut Option<Explanation>,
    ) -> Option<Amount> {
        let exchange_balance = self.get_exchange_balance(
            &balance_request.exchange_account_id,
            currency_pair_metadata.clone(),
            balance_request.currency_code,
            price,
        )?;

        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "get_virtual_balance exchange_balance = {}",
                exchange_balance
            ));
        }

        let current_balance_diff = if !currency_pair_metadata.is_derivative {
            self.balance_diff
                .get_by_balance_request(balance_request)
                .unwrap_or(dec!(0))
        } else {
            let price = price?;
            let balance_currency_code_request = BalanceRequest::new(
                balance_request.configuration_descriptor.clone(),
                balance_request.exchange_account_id,
                balance_request.currency_pair,
                currency_pair_metadata
                    .balance_currency_code
                    .expect("failed to create BalanceRequest: currency_pair_metadata.balance_currency_code is None"),
            );
            let balance_currency_code_balance_diff = self
                .balance_diff
                .get_by_balance_request(&balance_currency_code_request)
                .unwrap_or(dec!(0));

            if let Some(explanation) = explanation {
                explanation.add_reason(format!(
                    "get_virtual_balance balance_currency_code_balance_diff = {}",
                    balance_currency_code_balance_diff
                ));
            }

            let cur_balance_diff = currency_pair_metadata
                .convert_amount_from_balance_currency_code(
                    balance_request.currency_code,
                    balance_currency_code_balance_diff,
                    price,
                );

            if let Some(explanation) = explanation {
                explanation.add_reason(format!(
                    "get_virtual_balance current_balance_diff = {}",
                    cur_balance_diff
                ));
            }

            cur_balance_diff
        };
        return Some(exchange_balance + current_balance_diff);
    }

    pub fn get_exchange_balance(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        currency_code: CurrencyCode,
        price: Option<Price>,
    ) -> Option<Amount> {
        if !currency_pair_metadata.is_derivative
            || currency_pair_metadata.balance_currency_code == Some(currency_code)
        {
            return self.get_raw_exchange_balance(exchange_account_id, &currency_code);
        }

        let price = price?;

        let balance_currency_code_balance = self.get_raw_exchange_balance(
            exchange_account_id,
            &currency_pair_metadata
                .balance_currency_code
                .expect("failed to get exchange balance: balance_currency_code should be non None"),
        )?;

        Some(
            currency_pair_metadata.convert_amount_from_balance_currency_code(
                currency_code,
                balance_currency_code_balance,
                price,
            ),
        )
    }

    fn get_raw_exchange_balance(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_code: &CurrencyCode,
    ) -> Option<Amount> {
        self.balance_by_exchange_id
            .get(exchange_account_id)?
            .get(currency_code)
            .cloned()
    }

    pub fn get_raw_exchange_balances(&self) -> &BalanceByExchangeId {
        &self.balance_by_exchange_id
    }

    pub fn get_virtual_balance_diffs(&self) -> &ServiceValueTree {
        &self.balance_diff
    }

    pub fn has_real_balance_on_exchange(&self, exchange_account_id: &ExchangeAccountId) -> bool {
        self.balance_by_exchange_id
            .get(exchange_account_id)
            .map(|x| x.len() > 0)
            .unwrap_or(false)
    }
}
