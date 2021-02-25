use super::application_manager::ApplicationManager;
use super::common::{CurrencyPair, ExchangeError, ExchangeErrorType};
use super::common_interaction::*;
use crate::core::exchanges::cancellation_token::CancellationToken;
use crate::core::exchanges::common::{RestRequestOutcome, SpecificCurrencyPair};
use crate::core::orders::fill::EventSourceType;
use crate::core::orders::order::{ExchangeOrderId, OrderCancelling, OrderCreating, OrderInfo};
use crate::core::orders::pool::OrdersPool;
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::common::ExchangeAccountId,
};
use crate::core::{
    connectivity::{connectivity_manager::ConnectivityManager, websocket_actor::WebSocketParams},
    orders::order::ClientOrderId,
};
use awc::http::StatusCode;
use dashmap::DashMap;
use futures::{pin_mut, Future};
use log::info;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestResult {
    Success(ExchangeOrderId),
    Error(ExchangeError),
    // TODO for that we need match binance_error_code as number with ExchangeErrorType
    //Error(ExchangeErrorType),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CreateOrderResult {
    pub outcome: RequestResult,
    pub source_type: EventSourceType,
}

impl CreateOrderResult {
    pub fn successed(exchange_order_id: ExchangeOrderId, source_type: EventSourceType) -> Self {
        CreateOrderResult {
            outcome: RequestResult::Success(exchange_order_id),
            source_type,
        }
    }

    pub fn failed(error: ExchangeError, source_type: EventSourceType) -> Self {
        CreateOrderResult {
            outcome: RequestResult::Error(error),
            source_type,
        }
    }
}

type WSEventType = CreateOrderResult;
pub struct Exchange {
    exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    // TODO Why not inside exchange_interaction?
    websocket_channels: Vec<String>,
    exchange_interaction: Box<dyn CommonInteraction>,
    orders: Arc<OrdersPool>,
    connectivity_manager: Arc<ConnectivityManager>,

    // It allows to send and receive notification about event in websocket channel
    // Current codebase relies on notification thru websocket channel instead
    // E.g. that new order was succesfully created
    // Rest response using only for unsuccsessful operations as error
    websocket_events: DashMap<
        ClientOrderId,
        (
            oneshot::Sender<WSEventType>,
            Option<oneshot::Receiver<WSEventType>>,
        ),
    >,
    application_manager: ApplicationManager,
}

impl Exchange {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        websocket_channels: Vec<String>,
        exchange_interaction: Box<dyn CommonInteraction>,
    ) -> Arc<Self> {
        let connectivity_manager = ConnectivityManager::new(exchange_account_id.clone());

        let exchange = Arc::new(Self {
            exchange_account_id: exchange_account_id.clone(),
            websocket_host,
            specific_currency_pairs,
            websocket_channels,
            exchange_interaction,
            orders: OrdersPool::new(),
            connectivity_manager,
            websocket_events: DashMap::new(),
            // TODO in the future application_manager have to be passed as parameter
            application_manager: ApplicationManager::default(),
        });

        exchange.clone().setup_connectivity_manager();
        exchange.clone().setup_exchange_interaction();

        exchange
    }

    fn raise_order_created(
        &self,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        source_type: EventSourceType,
    ) {
        if let Some((_, (tx, _))) = self.websocket_events.remove(&client_order_id) {
            tx.send(CreateOrderResult::successed(exchange_order_id, source_type))
                .unwrap();
        }
    }

    fn setup_connectivity_manager(self: Arc<Self>) {
        let exchange_weak = Arc::downgrade(&self);
        self.connectivity_manager
            .set_callback_msg_received(Box::new(move |data| match exchange_weak.upgrade() {
                Some(exchange) => exchange.on_websocket_message(data),
                None => info!(
                    "Unable to upgrade weak referene to Exchange instance. Probably it's dead"
                ),
            }));
    }

    fn setup_exchange_interaction(self: Arc<Self>) {
        let exchange_weak = Arc::downgrade(&self);
        self.exchange_interaction
            .set_order_created_callback(Box::new(
                move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade()
                {
                    Some(exchange) => exchange.raise_order_created(
                        client_order_id,
                        exchange_order_id,
                        source_type,
                    ),
                    None => info!(
                        "Unable to upgrade weak referene to Exchange instance. Probably it's dead",
                    ),
                },
            ));
    }

    fn on_websocket_message(&self, msg: &str) {
        if self
            .application_manager
            .cancellation_token
            .check_cancellation_requested()
        {
            return;
        }

        if self.exchange_interaction.should_log_message(msg) {
            self.log_websocket_message(msg);
        }
        self.exchange_interaction.on_websocket_message(msg);
    }

    fn log_websocket_message(&self, msg: &str) {
        info!(
            "Websocket message from {}:{}: {}",
            self.exchange_account_id.exchange_id.as_str(),
            self.exchange_account_id.account_number,
            msg
        );
    }

    pub fn create_websocket_params(&self, ws_path: &str) -> WebSocketParams {
        WebSocketParams::new(
            format!("{}{}", self.websocket_host, ws_path)
                .parse()
                .expect("should be valid url"),
        )
    }

    pub async fn connect(self: Arc<Self>) {
        self.try_connect().await;
        // TODO Reconnect
    }

    async fn try_connect(self: Arc<Self>) {
        // TODO IsWebSocketConnecting()
        info!("Websocket: Connecting on {}", "test_exchange_id");

        // TODO if UsingWebsocket
        // TODO handle results

        let exchange_weak = Arc::downgrade(&self);
        let get_websocket_params = Box::new(move |websocket_role| {
            let exchange = exchange_weak.upgrade().unwrap();
            let params = exchange.get_websocket_params(websocket_role);
            // TODO Evgeniy, look at this. It works but also scares me a little
            Box::pin(params) as Pin<Box<dyn Future<Output = Option<WebSocketParams>>>>
        });

        let is_connected = self
            .connectivity_manager
            .clone()
            .connect(true, get_websocket_params)
            .await;

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
            return CreateOrderResult::failed(rest_error, EventSourceType::Rest);
        }

        let created_order_id = self.exchange_interaction.get_order_id(&request_outcome);
        CreateOrderResult::successed(created_order_id, EventSourceType::Rest)
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

    pub async fn create_order(
        &self,
        order: &OrderCreating,
        cancellation_token: CancellationToken,
    ) -> Option<CreateOrderResult> {
        let client_order_id = order.header.client_order_id.clone();
        let (tx, websocket_event_receiver) = oneshot::channel();

        self.websocket_events
            .insert(client_order_id.clone(), (tx, None));

        let order_create_future = self.exchange_interaction.create_order(&order);
        let cancellation_token = cancellation_token.when_cancelled();

        pin_mut!(order_create_future);
        pin_mut!(cancellation_token);
        pin_mut!(websocket_event_receiver);

        tokio::select! {
            rest_request_outcome = &mut order_create_future => {

                let create_order_result = self.handle_response(&rest_request_outcome, &order);
                match create_order_result.outcome {
                    RequestResult::Error(_) => {
                        // TODO if ExchangeFeatures.Order.CreationResponseFromRestOnlyForError
                        return Some(create_order_result);
                    }

                    RequestResult::Success(_) => {
                        tokio::select! {
                            websocket_outcome = &mut websocket_event_receiver => {
                                return Some(websocket_outcome.unwrap())
                            }

                            _ = &mut cancellation_token => {
                                return None;
                            }

                        }
                    }
                }
            }

            _ = &mut cancellation_token => {
                return None;
            }

            websocket_outcome = &mut websocket_event_receiver => {
                return Some(websocket_outcome.unwrap());
            }

        };
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
        // TODO some timer metric has to be here

        let response = self.exchange_interaction.get_open_orders().await;
        info!("GetOpenOrders response is {:?}", response);

        // TODO IsRestError(response) with Result?? Prolly just log error
        // TODO Result propagate and handling

        let orders = self.exchange_interaction.parse_open_orders(&response);

        orders
    }

    pub async fn get_websocket_params(
        self: Arc<Self>,
        websocket_role: WebSocketRole,
    ) -> Option<WebSocketParams> {
        let ws_path;
        match websocket_role {
            WebSocketRole::Main => {
                // TODO remove hardcode or probably extract to common_interaction trait
                ws_path = self.exchange_interaction.build_ws_main_path(
                    &self.specific_currency_pairs[..],
                    &self.websocket_channels[..],
                );
            }
            WebSocketRole::Secondary => {
                ws_path = self.exchange_interaction.build_ws_secondary_path().await;
            }
        }

        Some(self.create_websocket_params(&ws_path))
    }
}
