use super::cancellation_token;
use super::common::{CurrencyPair, ExchangeError, ExchangeErrorType};
use super::common_interaction::*;
use crate::core::connectivity::websocket_actor::WebSocketParams;
use crate::core::exchanges::binance::Binance;
use crate::core::exchanges::common::{RestRequestOutcome, SpecificCurrencyPair};
use crate::core::orders::order::{ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo};
use crate::core::orders::pool::OrdersPool;
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::common::ExchangeAccountId,
};
use actix::{Actor, Context, Handler, Message};
use awc::http::StatusCode;
use log::{info, trace};
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestResult {
    Success(ExchangeOrderId),
    // TODO for that we need match binance_error_code as number with ExchangeErrorType
    //Error(ExchangeErrorType),
    Error(ExchangeError),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CreateOrderResult {
    pub outcome: RequestResult,
    // Do not needed yet
    // pub source_type: EventSourceType
}

impl CreateOrderResult {
    pub fn successed(exchange_order_id: ExchangeOrderId, /*source_type: EventSourceType*/) -> Self {
        CreateOrderResult {
            outcome: RequestResult::Success(exchange_order_id),
            //source_type
        }
    }

    pub fn failed(error: ExchangeError /*source_type: EventSourceType*/) -> Self {
        CreateOrderResult {
            outcome: RequestResult::Error(error),
            //source_type
        }
    }
}

pub struct ExchangeActor {
    exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    websocket_channels: Vec<String>,
    exchange_interaction: Box<dyn CommonInteraction>,
    orders: Arc<OrdersPool>,
}

impl ExchangeActor {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        websocket_channels: Vec<String>,
        exchange_interaction: Box<dyn CommonInteraction>,
    ) -> Self {
        ExchangeActor {
            exchange_account_id,
            websocket_host,
            specific_currency_pairs,
            websocket_channels,
            exchange_interaction,
            orders: OrdersPool::new(),
        }
    }

    pub fn create_websocket_params(&mut self, ws_path: &str) -> WebSocketParams {
        WebSocketParams::new(
            format!("{}{}", self.websocket_host, ws_path)
                .parse()
                .expect("should be valid url"),
        )
    }

    fn handle_response(
        &self,
        request_outcome: &RestRequestOutcome,
        order: &OrderCreating,
    ) -> CreateOrderResult {
        info!(
            "Create response for {}, {:?}, {}, {:?}",
            // TODO other order_headers_field
            order.header.client_order_id,
            order.header.exchange_account_id.exchange_id,
            order.header.exchange_account_id.account_number,
            request_outcome
        );

        if let Some(rest_error) = self.is_rest_error_order(request_outcome, order) {
            return CreateOrderResult::failed(rest_error);
        }

        let created_order_id = self.exchange_interaction.get_order_id(&request_outcome);
        CreateOrderResult::successed(created_order_id)
    }

    pub fn is_rest_error_order(
        &self,
        response: &RestRequestOutcome,
        _order: &OrderCreating,
    ) -> Option<ExchangeError> {
        // TODO add log with info about caller
        match response.status {
            StatusCode::UNAUTHORIZED => {
                return Some(ExchangeError::new(
                    ExchangeErrorType::Authentication,
                    response.content.clone(),
                    None,
                ));
            }
            StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => {
                return Some(ExchangeError::new(
                    ExchangeErrorType::Authentication,
                    response.content.clone(),
                    None,
                ));
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Some(ExchangeError::new(
                    ExchangeErrorType::RateLimit,
                    response.content.clone(),
                    None,
                ));
            }
            _ => {
                if response.content.is_empty() {
                    return Some(ExchangeError::new(
                        ExchangeErrorType::Unknown,
                        "Empty response".to_owned(),
                        None,
                    ));
                }

                if let Some(error) = self.exchange_interaction.is_rest_error_code(&response) {
                    let error_type = self.exchange_interaction.get_error_type(&error);

                    return Some(ExchangeError::new(
                        error_type,
                        error.message,
                        Some(error.code),
                    ));
                }

                None
            }
        }
    }

    pub async fn create_order(&self, order: &OrderCreating) -> CreateOrderResult {
        let order_create_task = self.exchange_interaction.create_order(&order);
        let cancellation_token = cancellation_token::CancellationToken::when_cancelled();

        tokio::select! {
            rest_request_outcome = order_create_task => {

                let create_order_result = self.handle_response(&rest_request_outcome, &order);
                create_order_result

            }
            _ = cancellation_token => {
                unimplemented!();
            }
        }
    }

    pub async fn cancel_order(&self, order: &OrderCancelling) {
        self.exchange_interaction.cancel_order(&order).await;
    }

    pub async fn cancel_all_orders(&self, currency_pair: CurrencyPair) {
        self.exchange_interaction
            .cancel_all_orders(currency_pair)
            .await;
    }

    pub async fn get_account_info(&self) {
        self.exchange_interaction.get_account_info().await;
    }

    pub async fn get_open_orders(&self) -> Vec<OrderInfo> {
        // TODO add logging
        let response = self.exchange_interaction.get_open_orders().await;
        // TODO IsRestError(response) with Result?? Prolly just log error

        let orders = self.exchange_interaction.parse_open_orders(&response);

        orders
    }
}

impl Actor for ExchangeActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        trace!("ExchangeActor '{}' started", self.exchange_account_id);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        trace!("ExchangeActor '{}' stopped", self.exchange_account_id);
    }
}

pub struct GetWebSocketParams(pub WebSocketRole);

impl Message for GetWebSocketParams {
    type Result = Option<WebSocketParams>;
}

impl Handler<GetWebSocketParams> for ExchangeActor {
    type Result = Option<WebSocketParams>;

    fn handle(&mut self, msg: GetWebSocketParams, _ctx: &mut Self::Context) -> Self::Result {
        let websocket_role = msg.0;
        match websocket_role {
            WebSocketRole::Main => {
                // TODO remove hardcode
                let ws_path = Binance::build_ws1_path(
                    &self.specific_currency_pairs[..],
                    &self.websocket_channels[..],
                );
                Some(self.create_websocket_params(&ws_path))
            }
            WebSocketRole::Secondary => None,
        }
    }
}
