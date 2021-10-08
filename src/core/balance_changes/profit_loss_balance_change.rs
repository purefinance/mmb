use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use lazy_static::lazy_static;
use rust_decimal::Decimal;

use crate::core::{
    balance_manager::balance_request::BalanceRequest,
    exchanges::common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, ExchangeId, Price},
    orders::order::ClientOrderFillId,
    DateTime,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProfitLossBalanceChangeId(u64);

lazy_static! {
    static ref PROFIT_LOSS_BALANCE_CHANGE_ID: AtomicU64 = {
        AtomicU64::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get system time since UNIX_EPOCH")
                .as_secs(),
        )
    };
}

impl ProfitLossBalanceChangeId {
    pub fn generate() -> Self {
        let new_id = PROFIT_LOSS_BALANCE_CHANGE_ID.fetch_add(1, Ordering::AcqRel);
        Self(new_id)
    }
}

#[derive(Clone)]
pub(crate) struct ProfitLossBalanceChange {
    pub id: ProfitLossBalanceChangeId,
    pub client_order_fill_id: ClientOrderFillId,
    pub change_date: DateTime,
    pub service_name: String,
    pub service_configuration_key: String,
    pub exchange_id: ExchangeId,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
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
            service_name: request.configuration_descriptor.service_name.clone(),
            service_configuration_key: request
                .configuration_descriptor
                .service_configuration_key
                .clone(),
            exchange_id,
            exchange_account_id: request.exchange_account_id,
            currency_pair: request.currency_pair,
            currency_code: request.currency_code,
            balance_change,
            usd_price: usd_balance_change / balance_change,
            usd_balance_change: usd_balance_change,
        }
    }

    pub fn with_portion(&self, portion: Decimal) -> ProfitLossBalanceChange {
        let mut item = self.clone();
        item.balance_change *= portion;
        item.usd_balance_change *= portion;
        item
    }
}
