use std::collections::HashMap;

use crate::core::balance_manager::position_change::PositionChange;
use crate::core::exchanges::common::{CurrencyPair, ExchangeAccountId, TradePlaceAccount};
use crate::core::orders::order::ClientOrderId;
use crate::core::DateTime;

use anyhow::{bail, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Clone)]
pub(crate) struct BalancePositionByFillAmount {
    /// TradePlace -> AmountInAmountCurrency
    position_by_fill_amount: HashMap<TradePlaceAccount, Decimal>,

    /// TradePlace -> AmountInAmountCurrency
    position_changes: HashMap<TradePlaceAccount, Vec<PositionChange>>,
}

impl BalancePositionByFillAmount {
    pub fn get(
        &self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
    ) -> Option<Decimal> {
        self.position_by_fill_amount
            .get(&TradePlaceAccount::new(
                exchange_account_id.clone(),
                currency_pair.clone(),
            ))
            .cloned()
    }

    pub(crate) fn set(
        &mut self,
        exchange_account_id: &ExchangeAccountId,
        currency_pair: &CurrencyPair,
        pervious_position: Option<Decimal>,
        new_position: Decimal,
        client_order_fill_id: Option<ClientOrderId>,
        now: DateTime,
    ) -> Result<()> {
        let key = TradePlaceAccount::new(exchange_account_id.clone(), currency_pair.clone());

        log::info!(
            "PositionChanges {:?} {} {:?}",
            pervious_position,
            new_position,
            client_order_fill_id
        );

        //We don't come from RestoreFillAmountPosition
        if let (Some(pervious_position), Some(client_order_fill_id)) =
            (pervious_position, client_order_fill_id)
        {
            let position_change_contains_key = self.position_changes.contains_key(&key);
            log::info!("position_changes {}", position_change_contains_key);

            if position_change_contains_key {
                if (pervious_position.is_sign_negative() && new_position.is_sign_positive())
                    || (pervious_position.is_sign_positive() && new_position.is_sign_negative())
                    || (pervious_position == dec!(0) && new_position == dec!(0))
                {
                    let opened_position_portion = new_position / (new_position - pervious_position);
                    match self.position_changes.get_mut(&key) {
                        Some(position_changes) => position_changes.push(PositionChange::new(
                            client_order_fill_id.clone(),
                            now,
                            opened_position_portion,
                        )),
                        None => bail!(
                            "failed to get PositionChange from position_changes {:?} with key {:?}",
                            self.position_changes,
                            key
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
                if pervious_position != dec!(0) {
                    log::error!(
                        "_lostPositionOpenTime has no records but position is not zero {} {} {}",
                        key.exchange_account_id,
                        key.currency_pair,
                        pervious_position
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
        Ok(())
    }
}
