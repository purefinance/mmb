use std::hash::Hash;
use std::sync::Arc;

use crate::balance::manager::balance_reservation::BalanceReservation;
use crate::exchanges::common::{Amount, ExchangeAccountId, Price};
use crate::exchanges::general::symbol::Symbol;
use crate::orders::order::OrderSide;
use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;

#[derive(Clone, Hash, Debug, Eq, PartialEq)]
pub struct ReserveParameters {
    pub(crate) configuration_descriptor: ConfigurationDescriptor,
    pub(crate) exchange_account_id: ExchangeAccountId,
    pub(crate) symbol: Arc<Symbol>,
    pub(crate) order_side: OrderSide,
    pub(crate) price: Price,
    pub(crate) amount: Amount,
}

impl ReserveParameters {
    pub fn new(
        configuration_descriptor: ConfigurationDescriptor,
        exchange_account_id: ExchangeAccountId,
        symbol: Arc<Symbol>,
        order_side: OrderSide,
        price: Price,
        amount: Amount,
    ) -> Self {
        Self {
            configuration_descriptor,
            exchange_account_id,
            symbol,
            order_side,
            price,
            amount,
        }
    }

    pub fn from_reservation(reservation: &BalanceReservation, amount: Amount) -> Self {
        ReserveParameters::new(
            reservation.configuration_descriptor,
            reservation.exchange_account_id,
            reservation.symbol.clone(),
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
            symbol: reservation.symbol,
            order_side: reservation.order_side,
            price,
            amount,
        }
    }
}
