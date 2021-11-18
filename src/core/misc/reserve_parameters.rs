use std::hash::Hash;
use std::sync::Arc;

use crate::core::balance_manager::balance_reservation::BalanceReservation;
use crate::core::exchanges::common::{Amount, ExchangeAccountId, Price};
use crate::core::exchanges::general::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::orders::order::OrderSide;
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

#[derive(Clone, Hash, Debug, Eq, PartialEq)]
pub struct ReserveParameters {
    pub(crate) configuration_descriptor: ConfigurationDescriptor,
    pub(crate) exchange_account_id: ExchangeAccountId,
    pub(crate) currency_pair_metadata: Arc<CurrencyPairMetadata>,
    pub(crate) order_side: OrderSide,
    pub(crate) price: Price,
    pub(crate) amount: Amount,
}

impl ReserveParameters {
    pub fn new(
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        order_side: OrderSide,
        price: Price,
        amount: Amount,
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

    pub fn from_reservation(reservation: &BalanceReservation, amount: Amount) -> Self {
        ReserveParameters::new(
            reservation.configuration_descriptor.clone(),
            reservation.exchange_account_id,
            reservation.currency_pair_metadata.clone(),
            reservation.order_side,
            reservation.price,
            amount,
        )
    }
    pub fn new_by_balance_reservation(
        reservation: BalanceReservation,
        price: Price,
        amount: Amount,
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
}
