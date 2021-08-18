use std::collections::HashMap;

use crate::core::balance_manager::position_change::PositionChange;
use crate::core::exchanges::common::{CurrencyPair, ExchangeAccountId, TradePlaceAccount};

use rust_decimal::Decimal;

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
}
