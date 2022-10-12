use crate::bitmex::Bitmex;
use anyhow::{bail, Result};
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
        match self.do_create_order(order).await {
            Ok(request_outcome) => match self.get_order_id(&request_outcome) {
                Ok(order_id) => CreateOrderResult::succeed(&order_id, EventSourceType::Rest),
                Err(error) => CreateOrderResult::failed(error, EventSourceType::Rest),
            },
            Err(err) => CreateOrderResult::failed(err, EventSourceType::Rest),
        }
    }

    async fn cancel_order(&self, order: OrderCancelling) -> CancelOrderResult {
        let order_header = order.header.clone();

        match self.do_cancel_order(order).await {
            Ok(_) => CancelOrderResult::succeed(
                order_header.client_order_id.clone(),
                EventSourceType::Rest,
                None,
            ),
            Err(err) => CancelOrderResult::failed(err, EventSourceType::Rest),
        }
    }

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair) -> Result<()> {
        match self.do_cancel_all_orders().await {
            Ok(_) => Ok(()),
            Err(error) => bail!("Failed to cancel all orders: {error:?}"),
        }
    }

    async fn get_open_orders(&self) -> Result<Vec<OrderInfo>> {
        let response = self.request_open_orders(None).await?;

        self.parse_open_orders(&response)
    }

    async fn get_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        let response = self.request_open_orders(Some(currency_pair)).await?;

        self.parse_open_orders(&response)
    }

    async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        match self.request_order_info(order).await {
            Ok(request_outcome) => self.parse_order_info(&request_outcome).map_err(|err| {
                ExchangeError::parsing(format!("Unable to parse order info: {err:?}"))
            }),
            Err(error) => Err(ExchangeError::unknown(
                format!("Failed to get order info: {:?}", error).as_str(),
            )),
        }
    }

    async fn close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<ClosedPosition> {
        let response = self.request_close_position(position, price).await?;

        self.parse_close_position(&response)
    }

    async fn get_active_positions(&self) -> Result<Vec<ActivePosition>> {
        let response = self.request_get_position().await?;

        self.parse_get_position(&response)
    }

    async fn get_balance(&self) -> Result<ExchangeBalancesAndPositions> {
        let response = self.request_get_balance().await?;

        self.parse_get_balance(&response)
    }

    async fn get_balance_and_positions(&self) -> Result<ExchangeBalancesAndPositions> {
        let balance_response = self.request_get_balance().await?;
        let positions_response = self.request_get_position().await?;

        self.parse_balance_and_positions(&balance_response, &positions_response)
    }

    async fn get_my_trades(
        &self,
        symbol: &Symbol,
        last_date_time: Option<DateTime>,
    ) -> RequestResult<Vec<OrderTrade>> {
        match self.request_my_trades(symbol, last_date_time).await {
            Ok(response) => match self.parse_my_trades(&response) {
                Ok(data) => RequestResult::Success(data),
                Err(err) => RequestResult::Error(ExchangeError::parsing(format!(
                    "Unable to parse trades: {err:?}"
                ))),
            },
            Err(err) => RequestResult::Error(ExchangeError::unknown(
                format!("Failed to get trades: {err:?}").as_str(),
            )),
        }
    }

    async fn build_all_symbols(&self) -> Result<Vec<Arc<Symbol>>> {
        let response = self.request_all_symbols().await?;
        self.parse_all_symbols(&response)
    }
}
