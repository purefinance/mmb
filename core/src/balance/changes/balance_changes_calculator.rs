use mockall_double::double;

use domain::exchanges::symbol::Symbol;
use domain::order::fill::OrderFill;
use domain::order::snapshot::{OrderSide, OrderSnapshot};
use std::sync::Arc;

#[double]
use crate::exchanges::general::currency_pair_to_symbol_converter::CurrencyPairToSymbolConverter;

use crate::{
    balance::manager::balance_request::BalanceRequest, misc::service_value_tree::ServiceValueTree,
    service_configuration::configuration_descriptor::ConfigurationDescriptor,
};

use super::balance_change_calculator_result::BalanceChangesCalculatorResult;

pub(crate) struct BalanceChangesCalculator {
    currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>,
}
impl BalanceChangesCalculator {
    pub fn new(currency_pair_to_symbol_converter: Arc<CurrencyPairToSymbolConverter>) -> Self {
        Self {
            currency_pair_to_symbol_converter,
        }
    }

    pub fn get_balance_changes(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        order: &OrderSnapshot,
        order_fill: &OrderFill,
    ) -> BalanceChangesCalculatorResult {
        let symbol = self
            .currency_pair_to_symbol_converter
            .get_symbol(order.header.exchange_account_id, order.header.currency_pair);

        self.get_balance_changes_calculator_results(
            configuration_descriptor,
            order,
            order_fill,
            symbol,
        )
    }

    fn get_balance_changes_calculator_results(
        &self,
        configuration_descriptor: ConfigurationDescriptor,
        order: &OrderSnapshot,
        order_fill: &OrderFill,
        symbol: Arc<Symbol>,
    ) -> BalanceChangesCalculatorResult {
        let price = order_fill.price();
        let filled_amount = order_fill.amount() * symbol.amount_multiplier;
        let commission_amount = order_fill.commission_amount();

        let order_side = order.header.side;
        let exchange_account_id = order.header.exchange_account_id;

        let (new_base_amount, new_quote_amount) = if !symbol.is_derivative {
            match order_side {
                OrderSide::Sell => (
                    -filled_amount,
                    symbol.convert_amount_from_amount_currency_code(
                        symbol.quote_currency_code(),
                        filled_amount,
                        price,
                    ) - commission_amount,
                ),
                OrderSide::Buy => (
                    filled_amount - commission_amount,
                    symbol.convert_amount_from_amount_currency_code(
                        symbol.quote_currency_code(),
                        -filled_amount,
                        price,
                    ),
                ),
            }
        } else {
            let balance_currency_code = symbol
                .balance_currency_code
                .expect("Balance currency code isn't set");

            if balance_currency_code == symbol.base_currency_code {
                match order_side {
                    OrderSide::Sell => (
                        symbol.convert_amount_from_amount_currency_code(
                            symbol.base_currency_code(),
                            -filled_amount,
                            price,
                        ) - commission_amount,
                        filled_amount,
                    ),
                    OrderSide::Buy => (
                        symbol.convert_amount_from_amount_currency_code(
                            symbol.base_currency_code(),
                            filled_amount,
                            price,
                        ) - commission_amount,
                        -filled_amount,
                    ),
                }
            } else if balance_currency_code == symbol.quote_currency_code {
                match order_side {
                    OrderSide::Sell => (
                        -filled_amount,
                        symbol.convert_amount_from_amount_currency_code(
                            symbol.quote_currency_code(),
                            filled_amount,
                            price,
                        ) - commission_amount,
                    ),
                    OrderSide::Buy => (
                        filled_amount,
                        symbol.convert_amount_from_amount_currency_code(
                            symbol.quote_currency_code(),
                            -filled_amount,
                            price,
                        ) - commission_amount,
                    ),
                }
            } else {
                panic!(
                    "BalanceChangesCalculator::get_balance_changes_calculator_results: balance_currency_code({}) is wrong.",
                    balance_currency_code
                )
            }
        };

        let base_currency_code_request = BalanceRequest::new(
            configuration_descriptor,
            exchange_account_id,
            symbol.currency_pair(),
            symbol.base_currency_code(),
        );

        let quote_currency_code_request = BalanceRequest::new(
            configuration_descriptor,
            exchange_account_id,
            symbol.currency_pair(),
            symbol.quote_currency_code(),
        );

        let mut res_balance_changes = ServiceValueTree::default();
        res_balance_changes.set_by_balance_request(&base_currency_code_request, new_base_amount);
        res_balance_changes.set_by_balance_request(&quote_currency_code_request, new_quote_amount);

        BalanceChangesCalculatorResult::new(
            res_balance_changes,
            symbol.quote_currency_code(),
            price,
            exchange_account_id.exchange_id,
        )
    }
}
