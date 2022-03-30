use std::fmt;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

use mmb_utils::DateTime;
use mmb_utils::{impl_u64_id, time::get_atomic_current_secs};
use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::service_configuration::configuration_descriptor::ConfigurationDescriptor;
use crate::{
    balance_manager::balance_request::BalanceRequest,
    exchanges::common::{Amount, CurrencyCode, ExchangeId, MarketAccountId, Price},
    orders::order::ClientOrderFillId,
};

impl_u64_id!(ProfitLossBalanceChangeId);

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct ProfitLossBalanceChange {
    pub id: ProfitLossBalanceChangeId,
    pub client_order_fill_id: ClientOrderFillId,
    pub change_date: DateTime,
    pub configuration_descriptor: ConfigurationDescriptor,
    pub exchange_id: ExchangeId,
    pub market_account_id: MarketAccountId,
    pub currency_code: CurrencyCode,
    pub balance_change: Amount,
    pub usd_price: Price,
    pub usd_balance_change: Amount,
}

impl ProfitLossBalanceChange {
    pub fn new(
        request: BalanceRequest,
        exchange_id: ExchangeId,
        client_order_fill_id: ClientOrderFillId,
        change_date: DateTime,
        balance_change: Amount,
        usd_balance_change: Amount,
    ) -> Self {
        Self {
            id: ProfitLossBalanceChangeId::generate(),
            client_order_fill_id,
            change_date,
            configuration_descriptor: request.configuration_descriptor,
            exchange_id,
            market_account_id: MarketAccountId::new(
                request.exchange_account_id,
                request.currency_pair,
            ),
            currency_code: request.currency_code,
            balance_change,
            usd_price: usd_balance_change / balance_change,
            usd_balance_change,
        }
    }

    pub fn with_portion(&self, portion: Decimal) -> ProfitLossBalanceChange {
        let mut item = self.clone();
        item.balance_change *= portion;
        item.usd_balance_change *= portion;
        item
    }
}

#[cfg(test)]
impl PartialOrd for ProfitLossBalanceChange {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
impl Ord for ProfitLossBalanceChange {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.change_date.cmp(&other.change_date)
    }
}
