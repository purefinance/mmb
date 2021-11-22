use std::fmt;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    core::{
        balance_manager::balance_request::BalanceRequest,
        exchanges::common::{Amount, CurrencyCode, ExchangeId, Price, TradePlaceAccount},
        orders::order::ClientOrderFillId,
        service_configuration::configuration_descriptor::{ServiceConfigurationKey, ServiceName},
        utils::get_atomic_current_secs,
        DateTime,
    },
    impl_u64_id,
};

impl_u64_id!(ProfitLossBalanceChangeId);

#[derive(Clone, Debug)]
pub(crate) struct ProfitLossBalanceChange {
    pub id: ProfitLossBalanceChangeId,
    pub client_order_fill_id: ClientOrderFillId,
    pub change_date: DateTime,
    pub service_name: ServiceName,
    pub service_configuration_key: ServiceConfigurationKey,
    pub exchange_id: ExchangeId,
    pub trade_place: TradePlaceAccount,
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
            service_name: request.configuration_descriptor.service_name,
            service_configuration_key: request
                .configuration_descriptor
                .service_configuration_key
                .clone(),
            exchange_id,
            trade_place: TradePlaceAccount::new(request.exchange_account_id, request.currency_pair),
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
