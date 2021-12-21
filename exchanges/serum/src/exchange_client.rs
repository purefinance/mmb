use crate::serum::Serum;

use anyhow::Result;
use async_trait::async_trait;

use mmb_core::core::exchanges::common::{ActivePosition, CurrencyPair, Price, RestRequestOutcome};
use mmb_core::core::exchanges::general::symbol::Symbol;
use mmb_core::core::exchanges::traits::ExchangeClient;
use mmb_core::core::orders::order::{OrderCancelling, OrderCreating};
use mmb_core::core::orders::pool::OrderRef;

#[async_trait]
impl ExchangeClient for Serum {
    async fn request_all_symbols(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn create_order(&self, order: &OrderCreating) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_cancel_order(&self, order: &OrderCancelling) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()> {
        todo!()
    }

    async fn request_open_orders(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_order_info(&self, order: &OrderRef) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_my_trades(
        &self,
        symbol: &Symbol,
        last_date_time: Option<mmb_utils::DateTime>,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_get_position(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_get_balance_and_position(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_get_balance(&self) -> Result<RestRequestOutcome> {
        todo!()
    }

    async fn request_close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<RestRequestOutcome> {
        todo!()
    }
}
