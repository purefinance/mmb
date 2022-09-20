use crate::bitmex::Bitmex;
use anyhow::Result;
use async_trait::async_trait;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::exchanges::general::order::cancel::CancelOrderResult;
use mmb_core::exchanges::general::order::create::CreateOrderResult;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::traits::{ExchangeClient, ExchangeError};
use mmb_domain::events::ExchangeBalancesAndPositions;
use mmb_domain::exchanges::symbol::Symbol;
use mmb_domain::market::CurrencyPair;
use mmb_domain::order::fill::EventSourceType;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::{OrderCancelling, OrderInfo, Price};
use mmb_domain::position::{ActivePosition, ClosedPosition};
use mmb_utils::DateTime;
use std::sync::Arc;

#[async_trait]
impl ExchangeClient for Bitmex {
    async fn create_order(&self, order: &OrderRef) -> CreateOrderResult {
        match self.request_create_order(order).await {
            Ok(request_outcome) => match self.get_order_id(&request_outcome) {
                Ok(order_id) => CreateOrderResult::succeed(&order_id, EventSourceType::Rest),
                Err(error) => CreateOrderResult::failed(error, EventSourceType::Rest),
            },
            Err(err) => CreateOrderResult::failed(err, EventSourceType::Rest),
        }
    }

    async fn cancel_order(&self, _order: OrderCancelling) -> CancelOrderResult {
        todo!()
    }

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair) -> Result<()> {
        todo!()
    }

    async fn get_open_orders(&self) -> Result<Vec<OrderInfo>> {
        todo!()
    }

    async fn get_open_orders_by_currency_pair(
        &self,
        _currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        todo!()
    }

    async fn get_order_info(&self, _order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        todo!()
    }

    async fn close_position(
        &self,
        _position: &ActivePosition,
        _price: Option<Price>,
    ) -> Result<ClosedPosition> {
        todo!()
    }

    async fn get_active_positions(&self) -> Result<Vec<ActivePosition>> {
        todo!()
    }

    async fn get_balance(&self) -> Result<ExchangeBalancesAndPositions> {
        todo!()
    }

    async fn get_balance_and_positions(&self) -> Result<ExchangeBalancesAndPositions> {
        todo!()
    }

    async fn get_my_trades(
        &self,
        _symbol: &Symbol,
        _last_date_time: Option<DateTime>,
    ) -> RequestResult<Vec<OrderTrade>> {
        todo!()
    }

    async fn build_all_symbols(&self) -> Result<Vec<Arc<Symbol>>> {
        let response = self.request_all_symbols().await?;
        self.parse_all_symbols(&response)
    }
}
