use std::sync::Arc;

use crate::core::{
    balance_manager::balance_request::BalanceRequest,
    exchanges::general::{
        currency_pair_metadata::CurrencyPairMetadata,
        currency_pair_to_metadata_converter::CurrencyPairToMetadataConverter,
    },
    misc::service_value_tree::ServiceValueTree,
    orders::{fill::OrderFill, order::OrderSide, pool::OrderRef},
    service_configuration::configuration_descriptor::ConfigurationDescriptor,
};

use super::balance_change_calculator_result::BalanceChangesCalculatorResult;

pub(crate) struct BalanceChangesCalculator {
    currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter,
}
impl BalanceChangesCalculator {
    pub fn new(currency_pair_to_metadata_converter: CurrencyPairToMetadataConverter) -> Self {
        Self {
            currency_pair_to_metadata_converter,
        }
    }

    pub fn get_balance_changes(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order: &OrderRef,
        order_fill: OrderFill,
    ) -> BalanceChangesCalculatorResult {
        let metadata = self
            .currency_pair_to_metadata_converter
            .get_currency_pair_metadata(&order.exchange_account_id(), &order.currency_pair());
        self.get_balance_changes_calculator_results(
            configuration_descriptor,
            order,
            order_fill,
            metadata,
        )
    }

    fn get_balance_changes_calculator_results(
        &self,
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        order: &OrderRef,
        order_fill: OrderFill,
        metadata: Arc<CurrencyPairMetadata>,
    ) -> BalanceChangesCalculatorResult {
        let price = order_fill.price();
        let filled_amount = order_fill.amount() * metadata.amount_multiplier;
        let commission_amount = order_fill.commission_amount();

        let (new_base_amount, new_quote_amount) = if !metadata.is_derivative {
            match order.side() {
                OrderSide::Sell => (
                    -filled_amount,
                    metadata.convert_amount_from_amount_currency_code(
                        &metadata.quote_currency_code(),
                        filled_amount,
                        price,
                    ) - commission_amount,
                ),
                OrderSide::Buy => (
                    filled_amount - commission_amount,
                    metadata.convert_amount_from_amount_currency_code(
                        &metadata.quote_currency_code(),
                        -filled_amount,
                        price,
                    ),
                ),
            }
        } else {
            // REVIEW: тут может быть none это не ошибка в C# есть доп проверка в `else if`
            let balance_currency_code = metadata
                .balance_currency_code
                .as_ref()
                .expect("Balance currency code isn't set");

            if balance_currency_code == &metadata.base_currency_code {
                match order.side() {
                    OrderSide::Sell => (
                        metadata.convert_amount_from_amount_currency_code(
                            &metadata.base_currency_code(),
                            -filled_amount,
                            price,
                        ) - commission_amount,
                        filled_amount,
                    ),
                    OrderSide::Buy => (
                        metadata.convert_amount_from_amount_currency_code(
                            &metadata.base_currency_code(),
                            filled_amount,
                            price,
                        ) - commission_amount,
                        -filled_amount,
                    ),
                }
            } else {
                match order.side() {
                    OrderSide::Sell => (
                        -filled_amount,
                        metadata.convert_amount_from_amount_currency_code(
                            &metadata.quote_currency_code(),
                            filled_amount,
                            price,
                        ) - commission_amount,
                    ),
                    OrderSide::Buy => (
                        filled_amount,
                        metadata.convert_amount_from_amount_currency_code(
                            &metadata.quote_currency_code(),
                            -filled_amount,
                            price,
                        ) - commission_amount,
                    ),
                }
            }
        };

        //  продолдить с 124 строки
        let base_currency_code_request = BalanceRequest::new(
            configuration_descriptor.clone(),
            order.exchange_account_id(),
            metadata.currency_pair(),
            metadata.base_currency_code(),
        );

        let quote_currency_code_request = BalanceRequest::new(
            configuration_descriptor,
            order.exchange_account_id(),
            metadata.currency_pair(),
            metadata.quote_currency_code(),
        );

        let mut res_balance_changes = ServiceValueTree::new();
        res_balance_changes.set_by_balance_request(&base_currency_code_request, new_base_amount);
        res_balance_changes.set_by_balance_request(&quote_currency_code_request, new_quote_amount);

        BalanceChangesCalculatorResult::new(
            res_balance_changes,
            metadata.quote_currency_code(),
            price,
            order.exchange_account_id().exchange_id,
        )
    }
}
