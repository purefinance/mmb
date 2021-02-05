use super::cancellation_token;
use super::common::CurrencyPair;
use super::common_interaction::*;
use crate::core::connectivity::websocket_actor::WebSocketParams;
use crate::core::exchanges::binance::Binance;
use crate::core::exchanges::common::{RestRequestOutcome, SpecificCurrencyPair};
use crate::core::orders::order::{ExchangeOrderId, OrderCancelling, OrderCreating};
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::common::ExchangeAccountId,
};
use actix::{Actor, Context, Handler, Message};
use awc::http::StatusCode;
use log::info;
use log::trace;
use serde_json::Value;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestResult {
    Success(ExchangeOrderId),
    // TODO for that we need match binance_error_code as number with ExchangeErrorType
    //Error(ExchangeErrorType),
    Error(i64),
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

    pub fn failed(error: i64 /*source_type: EventSourceType*/) -> Self {
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
        request_outcome: &RestRequestOutcome,
        order: &OrderCreating,
    ) -> CreateOrderResult {
        info!(
            "Create response for {}, {}, {}, {:?}",
            order.client_order_id, order.client_order_id, order.client_order_id, request_outcome
        );

        // TODO Here has to be more complete HTTP code handling
        if request_outcome.status == StatusCode::OK {
            let response: Value = serde_json::from_str(&request_outcome.content).unwrap();
            CreateOrderResult::successed(response["orderId"].to_string().as_str().into())
        } else {
            let error_description =
                Binance::get_error_description(&request_outcome.content).unwrap();
            CreateOrderResult::failed(error_description.code)
        }
    }

    pub async fn create_order(&self, order: &OrderCreating) -> CreateOrderResult {
        let order_create_task = self.exchange_interaction.create_order(&order);
        let cancellation_token = cancellation_token::CancellationToken::when_cancelled();

        tokio::select! {
            rest_request_outcome = order_create_task => {

                let create_order_result = Self::handle_response(&rest_request_outcome, &order);
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
