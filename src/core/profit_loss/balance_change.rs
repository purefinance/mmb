use rust_decimal::Decimal;
use uuid::Uuid;

use crate::core::{
    balance_manager::balance_request::BalanceRequest,
    exchanges::common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, ExchangeId, Price},
    orders::order::ClientOrderFillId,
    DateTime,
};

#[derive(Clone)]
pub struct ProfitLossBalanceChange {
    pub(crate) id: Uuid,
    pub client_order_fill_id: ClientOrderFillId,
    pub change_date: DateTime,
    pub service_name: String,
    pub service_configuration_key: String,
    pub exchange_account_id: ExchangeAccountId,
    pub exchange_name: ExchangeId,
    pub currency_pair: CurrencyPair,
    pub currency_code: CurrencyCode,
    pub balance_change: Amount,
    pub usd_price: Price,
    pub usd_balance_change: Amount,
}

impl ProfitLossBalanceChange {
    pub fn new(
        request: &BalanceRequest,
        exchange_name: ExchangeId,
        client_order_fill_id: ClientOrderFillId,
        change_date: DateTime,
        balance_change: Amount,
        usd_balance_change: Amount,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            client_order_fill_id,
            change_date,
            service_name: request.configuration_descriptor.service_name.clone(),
            service_configuration_key: request
                .configuration_descriptor
                .service_configuration_key
                .clone(),
            exchange_account_id: request.exchange_account_id.clone(),
            exchange_name,
            currency_pair: request.currency_pair.clone(),
            currency_code: request.currency_code.clone(),
            balance_change,
            usd_price: usd_balance_change / balance_change,
            usd_balance_change,
        }
    }

    pub fn clone_portion(&self, portion: Decimal) -> Self {
        let mut item = self.clone();
        item.balance_change *= portion;
        item.usd_balance_change *= portion;
        item
    }
}
