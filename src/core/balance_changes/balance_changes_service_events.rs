use crate::core::{orders::order::ClientOrderFillId, DateTime};

use super::balance_change_calculator_result::BalanceChangesCalculatorResult;

pub(crate) enum BalanceChangeServiceEvent {
    OnTimer,
    BalanceChange(BalanceChange),
}

pub(crate) struct BalanceChange {
    pub balance_changes: BalanceChangesCalculatorResult,
    pub client_order_fill_id: ClientOrderFillId,
    pub change_date: DateTime,
}

impl BalanceChange {
    pub fn new(
        balance_changes: BalanceChangesCalculatorResult,
        client_order_fill_id: ClientOrderFillId,
        change_date: DateTime,
    ) -> Self {
        Self {
            balance_changes,
            client_order_fill_id,
            change_date,
        }
    }
}
