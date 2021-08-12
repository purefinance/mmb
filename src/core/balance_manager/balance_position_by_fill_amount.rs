use std::collections::HashMap;

use crate::core::balance_manager::position_change::PositionChange;
use crate::core::exchanges::common::TradePlaceAccount;

use rust_decimal::Decimal;
pub(crate) struct BalancePositionByFillAmount {
    /// TradePlace -> AmountInAmountCurrency
    position_by_fill_amount: HashMap<TradePlaceAccount, Decimal>,

    /// TradePlace -> AmountInAmountCurrency
    position_changes: HashMap<TradePlaceAccount, Vec<PositionChange>>,
}
