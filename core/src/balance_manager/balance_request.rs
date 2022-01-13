use std::hash::Hash;

use crate::exchanges::common::ExchangeAccountId;
use crate::exchanges::common::{CurrencyCode, CurrencyPair};
use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;

use super::balance_reservation::BalanceReservation;

#[derive(Hash, Debug, PartialEq, Eq, Clone)]
/// The entity for getting balance for account with ExchangeAccountId by CurrencyPair in CurrencyCode
pub struct BalanceRequest {
    pub configuration_descriptor: ConfigurationDescriptor,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub currency_code: CurrencyCode,
}

impl BalanceRequest {
    pub fn new(
        configuration_descriptor: ConfigurationDescriptor,
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
            reservation.configuration_descriptor,
            reservation.exchange_account_id,
            reservation.symbol.currency_pair(),
            reservation.reservation_currency_code,
        )
    }
}
