use std::hash::Hash;
use std::sync::Arc;

use crate::core::exchanges::common::ExchangeAccountId;
use crate::core::exchanges::common::{CurrencyCode, CurrencyPair};
use crate::core::service_configuration::configuration_descriptor::ConfigurationDescriptor;

use super::balance_reservation::BalanceReservation;

#[derive(Hash, Debug, PartialEq, Clone)]
/// The entity for getting balance for account with ExchangeAccountId by CurrencyPair in CurrencyCode
pub struct BalanceRequest {
    pub configuration_descriptor: Arc<ConfigurationDescriptor>,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub currency_code: CurrencyCode,
}

impl BalanceRequest {
    pub fn new(
        configuration_descriptor: Arc<ConfigurationDescriptor>,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        currency_code: CurrencyCode,
    ) -> Self {
        Self {
            configuration_descriptor,
            exchange_account_id,
            currency_pair,
            currency_code,
        }
    }

    pub fn from_reservation(reservation: &BalanceReservation) -> Self {
        BalanceRequest::new(
            reservation.configuration_descriptor.clone(),
            reservation.exchange_account_id.clone(),
            reservation.currency_pair_metadata.currency_pair(),
            reservation.reservation_currency_code.clone(),
        )
    }
}

impl Eq for BalanceRequest {}
