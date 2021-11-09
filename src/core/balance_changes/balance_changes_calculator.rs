use mockall_double::double;

use std::sync::Arc;

#[double]
use crate::core::exchanges::general::currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter;

use crate::core::{
    balance_manager::balance_request::BalanceRequest,
    exchanges::general::currency_pair_metadata::CurrencyPairMetadata,
    misc::service_value_tree::ServiceValueTree,
    orders::{fill::OrderFill, order::OrderSide, pool::OrderRef},
    service_configuration::configuration_descriptor::ConfigurationDescriptor,
};

use super::balance_change_calculator_result::BalanceChangesCalculatorResult;

pub(crate) struct BalanceChangesCalculator {
    currency_pair_to_symbol_converter: Arc<CurrencyPairToMetadataConverter>,
}
impl BalanceChangesCalculator {
    pub fn new(currency_pair_to_symbol_converter: Arc<CurrencyPairToMetadataConverter>) -> Self {
        Self {
            currency_pair_to_symbol_converter,
        }
    }

    pub fn get_balance_changes(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order: &OrderRef,
        order_fill: OrderFill,
    ) -> BalanceChangesCalculatorResult {
        let (currency_pair, exchange_account_id) =
            order.fn_ref(|x| (x.header.currency_pair, x.header.exchange_account_id));

        let symbol = self
            .currency_pair_to_symbol_converter
            .get_currency_pair_metadata(exchange_account_id, currency_pair);

        self.get_balance_changes_calculator_results(
            configuration_descriptor,
            order,
            order_fill,
            symbol,
        )
    }

    fn get_balance_changes_calculator_results(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order: &OrderRef,
        order_fill: OrderFill,
        symbol: Arc<CurrencyPairMetadata>,
    ) -> BalanceChangesCalculatorResult {
        let price = order_fill.price();
        let filled_amount = order_fill.amount() * symbol.amount_multiplier;
        let commission_amount = order_fill.commission_amount();

        let (order_side, exchange_account_id) =
            order.fn_ref(|x| (x.header.side, x.header.exchange_account_id));

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
            configuration_descriptor.clone(),
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

        let mut res_balance_changes = ServiceValueTree::new();
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
