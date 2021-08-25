use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rust_decimal::Decimal;

use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::exchanges::common::{Amount, ExchangeAccountId};
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::orders::order::OrderSide;
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

#[derive(Clone, Hash, Debug, Eq, PartialEq)]
pub(crate) struct ReserveParameters {
    pub(crate) configuration_descriptor: ConfigurationDescriptor,
    pub(crate) exchange_account_id: ExchangeAccountId,
    pub(crate) currency_pair_metadata: Arc<CurrencyPairMetadata>,
    pub(crate) order_side: OrderSide,
    pub(crate) price: Decimal,
    pub(crate) amount: Amount,
}

impl ReserveParameters {
    pub fn new(
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        order_side: OrderSide,
        price: Decimal,
        amount: Decimal,
    ) -> Self {
        Self {
            configuration_descriptor,
            exchange_account_id,
            currency_pair_metadata,
            order_side,
            price,
            amount,
        }
    }

    pub fn new_by_balance_reservation(
        reservation: BalanceReservation,
        price: Decimal,
        amount: Decimal,
    ) -> Self {
        Self {
            configuration_descriptor: reservation.configuration_descriptor,
            exchange_account_id: reservation.exchange_account_id,
            currency_pair_metadata: reservation.currency_pair_metadata,
            order_side: reservation.order_side,
            price,
            amount,
        }
    }

    pub fn get_hash_code(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
