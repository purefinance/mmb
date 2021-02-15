// TODO rename file
use super::cancellation_token;
use super::common::{CurrencyPair, ExchangeError, ExchangeErrorType};
use super::common_interaction::*;
use crate::core::connectivity::{
    connectivity_manager::ConnectivityManager, websocket_actor::WebSocketParams,
};
use crate::core::exchanges::binance::Binance;
use crate::core::exchanges::common::{RestRequestOutcome, SpecificCurrencyPair};
use crate::core::orders::order::{ExchangeOrderId, OrderCancelling, OrderCreating};
use crate::core::orders::pool::OrdersPool;
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::common::ExchangeAccountId,
};
use actix::{Actor, Context, Handler, Message};
use awc::http::StatusCode;
use dashmap::DashMap;
use log::{info, trace};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;

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

type WSEventType = u32;

// TODO Dicided it's not an actor anymore
pub struct ExchangeActor {
    exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    websocket_channels: Vec<String>,
    exchange_interaction: Box<dyn CommonInteraction>,
    orders: Arc<OrdersPool>,
    connectivity_manager: Arc<ConnectivityManager>,
    websocket_events: DashMap<
        String,
        (
            oneshot::Sender<WSEventType>,
            Option<oneshot::Receiver<WSEventType>>,
        ),
    >,
}

impl ExchangeActor {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        // TODO why? For what?
        websocket_channels: Vec<String>,
        exchange_interaction: Box<dyn CommonInteraction>,
    ) -> Self {
        // TODO make it via DI to easier tests
        let connectivity_manager = Self::setup_connectivity_manager();
        let exchange = ExchangeActor {
            exchange_account_id,
            websocket_host,
            specific_currency_pairs,
            websocket_channels,
            exchange_interaction,
            orders: OrdersPool::new(),
            connectivity_manager: connectivity_manager.clone(),
            websocket_events: DashMap::new(),
        };

        connectivity_manager.set_callback_msg_received(Box::new(move |data| {
            Arc::new(exchange).clone().on_websocket_message(data)
        }));

        exchange
    }

    fn setup_connectivity_manager() -> Arc<ConnectivityManager> {
        // TODO set callbacks
        let connectivity_manager =
            ConnectivityManager::new(ExchangeAccountId::new("test_exchange_id".into(), 1));

        connectivity_manager
    }

    fn on_websocket_message(&self, msg: String) {
        // FIXME check cancellation token
        // FIXME check logging
        //exchange_interaction.on_websocket_message
        dbg!(&msg);
    }

    pub fn create_websocket_params(&mut self, ws_path: &str) -> WebSocketParams {
        WebSocketParams::new(
            format!("{}{}", self.websocket_host, ws_path)
                .parse()
                .expect("should be valid url"),
        )
    }

    pub async fn connect(&self) {
        self.try_connect().await;
        // TODO Reconnect
    }

    async fn try_connect(&self) {
        // TODO IsWebSocketConnecting()
        info!("Websocket: Connecting on {}", "test_exchange_id");

        // TODO if UsingWebsocket
        // TODO handle results
        // TODO handle secondarywebsocket

        let is_connected = self.connectivity_manager.connect(true).await;

        if !is_connected {
            // TODO finish_connected
        }
        // TODO all other logs and finish_connected
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
        let test_client_order_id = "test_id".to_string();
        let (tx, rx) = oneshot::channel();

        self.websocket_events
            .insert(test_client_order_id.clone(), (tx, Some(rx)));

        let (_, (tx, websocket_event_receiver)) =
            self.websocket_events.remove(&test_client_order_id).unwrap();
        tx.send(3);

        //let order_create_task = self.exchange_interaction.create_order(&order);
        let order_create_task = cancellation_token::CancellationToken::when_cancelled();
        let cancellation_token = cancellation_token::CancellationToken::when_cancelled();

        tokio::select! {
            //rest_request_outcome = order_create_task => {

            //    let create_order_result = self.handle_response(&rest_request_outcome, &order);
            //    create_order_result

            //}

            _ = order_create_task => {
                unimplemented!();
            }

            _ = cancellation_token => {
                unimplemented!();
            }

            websocket_outcome = websocket_event_receiver.unwrap() => {
                dbg!(&websocket_outcome);
                CreateOrderResult::successed("some_order_id".into())

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::settings::ExchangeSettings;

    #[actix_rt::test]
    async fn callback() {
        let exchange_account_id: ExchangeAccountId = "Binance0".parse().unwrap();
        let websocket_host = "wss://stream.binance.com:9443".into();
        let currency_pairs = vec!["bnbbtc".into(), "btcusdt".into()];
        let channels = vec!["depth".into(), "aggTrade".into()];
        let exchange_interaction = Box::new(Binance::new(
            ExchangeSettings::default(),
            exchange_account_id.clone(),
        ));

        let exchange_actor = ExchangeActor::new(
            exchange_account_id.clone(),
            websocket_host,
            currency_pairs,
            channels,
            exchange_interaction,
        );

        exchange_actor.connect().await;
    }
}
