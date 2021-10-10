use std::collections::HashMap;

use crate::core::balance_manager::position_change::PositionChange;
use crate::core::exchanges::common::{CurrencyPair, ExchangeAccountId, TradePlaceAccount};
use crate::core::orders::order::ClientOrderFillId;
use crate::core::DateTime;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Clone, Debug)]
pub struct BalancePositionByFillAmount {
    /// TradePlace -> AmountInAmountCurrency
    position_by_fill_amount: HashMap<TradePlaceAccount, Decimal>,

    /// TradePlace -> AmountInAmountCurrency
    position_changes: HashMap<TradePlaceAccount, Vec<PositionChange>>,
}

impl BalancePositionByFillAmount {
    pub fn new() -> Self {
        Self {
            position_by_fill_amount: HashMap::new(),
            position_changes: HashMap::new(),
        }
    }

    pub fn get(
        &self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
    ) -> Option<Decimal> {
        self.position_by_fill_amount
            .get(&TradePlaceAccount::new(exchange_account_id, currency_pair))
            .cloned()
    }

    pub(crate) fn set(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        previous_position: Option<Decimal>,
        new_position: Decimal,
        client_order_fill_id: Option<ClientOrderFillId>,
        now: DateTime,
    ) {
        let key = TradePlaceAccount::new(exchange_account_id.clone(), currency_pair.clone());

        log::info!(
            "PositionChanges {:?} {} {:?}",
            previous_position,
            new_position,
            client_order_fill_id
        );

        //We don't come from RestoreFillAmountPosition
        if let (Some(previous_position), Some(client_order_fill_id)) =
            (previous_position, client_order_fill_id)
        {
            let position_change_contains_key = self.position_changes.contains_key(&key);
            log::info!("position_changes {}", position_change_contains_key);

            if position_change_contains_key {
                if (previous_position.is_sign_negative() && new_position.is_sign_positive())
                    || (previous_position.is_sign_positive() && new_position.is_sign_negative())
                    || new_position.is_zero()
                {
                    let opened_position_portion = new_position / (new_position - previous_position);
                    match self.position_changes.get_mut(&key) {
                        Some(position_changes) => position_changes.push(PositionChange::new(
                            client_order_fill_id.clone(),
                            now,
                            opened_position_portion,
                        )),
                        None => panic!(
                            "failed to get PositionChange from position_changes {:?} with key {:?}",
                            self.position_changes, key
                        ),
                    }
                    log::info!(
                        "PositionChange was added {}  {} {} {} {}",
                        exchange_account_id,
                        currency_pair,
                        client_order_fill_id,
                        now,
                        opened_position_portion
                    );
                }
            } else {
                if !previous_position.is_zero() {
                    log::error!(
                        "_lostPositionOpenTime has no records but position is not zero {} {} {}",
                        key.exchange_account_id,
                        key.currency_pair,
                        previous_position
                    );
                }
                log::info!(
                    "PositionChange1 was added initially {} {} {} {}",
                    exchange_account_id,
                    currency_pair,
                    client_order_fill_id,
                    now
                );

                self.position_changes.insert(
                    key.clone(),
                    vec![PositionChange::new(client_order_fill_id, now, dec!(1))],
                );
            }
            if let Some(position_change) = self.position_changes.get(&key) {
                log::info!("PositionChanges {:?}", position_change);
            } else {
                log::warn!("PositionChanges for key {:?} not found", key);
            }
        }
        self.position_by_fill_amount.insert(key, new_position);
    }

    pub fn add(
        &mut self,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        value_to_add: Decimal,
        client_order_fill_id: Option<ClientOrderFillId>,
        now: DateTime,
    ) {
        let current_value = self
            .get(exchange_account_id, currency_pair)
            .unwrap_or_default();
        let new_value = current_value + value_to_add;
        self.set(
            exchange_account_id,
            currency_pair,
            Some(current_value),
            new_value,
            client_order_fill_id,
            now,
        )
    }

    pub fn get_last_position_change_before_period(
        &self,
        trade_place: &TradePlaceAccount,
        start_of_period: DateTime,
    ) -> Option<PositionChange> {
        if let Some(values) = self.position_changes.get(trade_place) {
            log::info!(
                "get_last_position_change_before_period get {} {} {:?}",
                trade_place.exchange_account_id,
                trade_place.currency_pair,
                values
            );

            let position_change = values
                .iter()
                .rfind(|&x| x.change_time <= start_of_period)
                .cloned();

            log::info!(
                "get_last_position_change_before_period {:?}",
                position_change,
            );
            return position_change;
        }
        log::info!(
            "get_last_position_change_before_period {} {} {:?}",
            trade_place.exchange_account_id,
            trade_place.currency_pair,
            self.position_changes.keys(),
        );
        None
    }
}
