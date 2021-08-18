use std::collections::HashMap;

use crate::core::balance_manager::balance_request::BalanceRequest;
use crate::core::exchanges::common::{CurrencyCode, ExchangeAccountId};
use crate::core::exchanges::general::currency_pair_metadata::{BeforeAfter, CurrencyPairMetadata};
use crate::core::explanation::Explanation;
use crate::core::misc::service_value_tree::ServiceValueTree;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

type BalanceByExchangeId = HashMap<ExchangeAccountId, HashMap<CurrencyCode, Decimal>>;

pub(crate) struct VirtualBalanceHolder {
    balance_by_exchange_id: BalanceByExchangeId,
    balance_diff: ServiceValueTree,
}

impl VirtualBalanceHolder {
    pub fn update_balances(
        &mut self,
        exchange_account_id: &ExchangeAccountId,
        balances_by_currency_code: &HashMap<CurrencyCode, Decimal>,
    ) {
        self.balance_by_exchange_id.insert(
            exchange_account_id.clone(),
            balances_by_currency_code.clone(),
        );

        log::info!(
            "VirtualBalanceHolder UpdateBalances {} {:?}",
            exchange_account_id,
            balances_by_currency_code
        );

        let all_diffs = self.balance_diff.get_as_balances();

        for currency_code in balances_by_currency_code.keys() {
            let balance_requests_to_clear = all_diffs.keys().map(|x| {
                if &x.exchange_account_id == exchange_account_id
                    && x.currency_code == currency_code.clone()
                {
                    return Some(x);
                }
                None
            });

            for balance_request in balance_requests_to_clear {
                match balance_request {
                    Some(balance_request) => {
                        self.balance_diff
                            .set_by_balance_request(balance_request, dec!(0));
                        log::info!(
                            "VirtualBalanceHolder update_balances Reset {} {}",
                            balance_request.exchange_account_id,
                            balance_request.currency_code
                        );
                    }
                    None => (),
                }
            }
        }
    }

    pub fn add_balance(
        &mut self,
        balance_request: &BalanceRequest,
        balance_to_add: Decimal,
        member_name: Option<String>,
    ) {
        let current_diff_value = self
            .balance_diff
            .get_by_balance_request(balance_request)
            .unwrap_or(dec!(0));
        let new_value = current_diff_value + balance_to_add;
        self.balance_diff
            .set_by_balance_request(balance_request, new_value);

        log::info!(
            "VirtualBalanceHolder add_balance {} {} {} {} {} {} {}",
            member_name.unwrap_or(String::from("")),
            balance_request.exchange_account_id,
            balance_request.currency_pair,
            balance_request.currency_code,
            current_diff_value,
            balance_to_add,
            new_value
        );
    }

    pub fn get_virtual_balance(
        &self,
        balance_request: &BalanceRequest,
        currency_pair_metadata: &CurrencyPairMetadata,
        price: Option<Decimal>,
        explanation: &mut Option<Explanation>,
    ) -> Option<Decimal> {
        let exchange_balance = self.get_exchange_balance(
            &balance_request.exchange_account_id,
            currency_pair_metadata,
            &balance_request.currency_code,
            price,
        )?;

        if let Some(explanation) = explanation {
            explanation.add_reason(format!(
                "get_virtual_balance exchange_balance = {}",
                exchange_balance
            ));
        }

        let current_balance_diff = if !currency_pair_metadata.is_derivative.clone() {
            self.balance_diff
                .get_by_balance_request(balance_request)
                .unwrap_or(dec!(0))
        } else {
            if price.is_none() {
                return None;
            }
            let balance_currnecy_code_request = BalanceRequest::new(
                balance_request.configuration_descriptor.clone(),
                balance_request.exchange_account_id.clone(),
                balance_request.currency_pair.clone(),
                currency_pair_metadata.balance_currency_code.clone()?,
            );
            let balance_currency_code_balance_diff = self
                .balance_diff
                .get_by_balance_request(&balance_currnecy_code_request)
                .unwrap_or(dec!(0));

            if let Some(explanation) = explanation {
                explanation.add_reason(format!(
                    "get_virtual_balance balance_currency_code_balance_diff = {}",
                    balance_currency_code_balance_diff
                ));
            }

            let cur_balance_diff = match currency_pair_metadata
                .convert_amount_from_balance_currency_code(
                    balance_request.currency_code.clone(),
                    balance_currency_code_balance_diff,
                    price?,
                ) {
                Ok(cur_balance_diff) => cur_balance_diff,
                Err(error) => {
                    log::error!(
                        "failed to convert amount from balance currency code: {:?}",
                        error
                    );
                    return None;
                }
            };

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
        currency_pair_metadata: &CurrencyPairMetadata,
        currency_code: &CurrencyCode,
        price: Option<Decimal>,
    ) -> Option<Decimal> {
        if currency_pair_metadata.is_derivative
            || currency_pair_metadata.balance_currency_code == Some(currency_code.clone())
        {
            return self.get_raw_exchange_balance(exchange_account_id, currency_code);
        }

        let balance_currency_code_balance = self.get_raw_exchange_balance(
            exchange_account_id,
            &currency_pair_metadata.balance_currency_code.clone()?,
        )?;

        let exchange_balance_in_currency_code = currency_pair_metadata
            .convert_amount_from_balance_currency_code(
                currency_code.clone(),
                balance_currency_code_balance,
                price?,
            );

        match exchange_balance_in_currency_code {
            Ok(exchange_balance_in_currency_code) => {
                return Some(exchange_balance_in_currency_code)
            }
            Err(error) => {
                log::error!(
                    "failed to convert amount from balance currency code: {:?}",
                    error
                );
            }
        }

        None
    }

    fn get_raw_exchange_balance(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_code: &CurrencyCode,
    ) -> Option<Decimal> {
        self.balance_by_exchange_id
            .get(exchange_account_id)?
            .get(currency_code)
            .cloned()
    }
}
