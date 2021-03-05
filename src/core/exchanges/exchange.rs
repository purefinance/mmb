use super::common::{Amount, CurrencyPair, ExchangeError, ExchangeErrorType};
use super::exchange_features::ExchangeFeatures;
use super::traits::ExchangeClient;
use super::{application_manager::ApplicationManager, exchange_features::OpenOrdersType};
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
use anyhow::*;
use awc::http::StatusCode;
use dashmap::DashMap;
use futures::{pin_mut, Future};
use log::{info, warn, Level};
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestResult<T> {
    Success(T),
    Error(ExchangeError),
    // TODO for that we need match binance_error_code as number with ExchangeErrorType
    //Error(ExchangeErrorType),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CreateOrderResult {
    pub outcome: RequestResult<ExchangeOrderId>,
    pub source_type: EventSourceType,
}

impl CreateOrderResult {
    pub fn successed(order_id: ExchangeOrderId, source_type: EventSourceType) -> Self {
        let test = RequestResult::Success(order_id);
        CreateOrderResult {
            outcome: test,
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct CancelOrderResult {
    pub outcome: RequestResult<ClientOrderId>,
    pub source_type: EventSourceType,
    // TODO Use in in the future
    pub filled_amount: Option<Amount>,
}

impl CancelOrderResult {
    pub fn successed(
        client_order_id: ClientOrderId,
        source_type: EventSourceType,
        filled_amount: Option<Amount>,
    ) -> Self {
        CancelOrderResult {
            outcome: RequestResult::Success(client_order_id),
            source_type,
            filled_amount,
        }
    }

    pub fn failed(error: ExchangeError, source_type: EventSourceType) -> Self {
        CancelOrderResult {
            outcome: RequestResult::Error(error),
            source_type,
            filled_amount: None,
        }
    }
}

enum CheckContent {
    Empty,
    Err(ExchangeError),
    Usable,
}

type CreationEventType = CreateOrderResult;
type CancelaltionEventType = CancelOrderResult;
pub struct Exchange {
    exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    websocket_channels: Vec<String>,
    exchange_interaction: Box<dyn ExchangeClient>,
    orders: Arc<OrdersPool>,
    connectivity_manager: Arc<ConnectivityManager>,

    // It allows to send and receive notification about event in websocket channel
    // Websocket event is main source detecting order creation result
    // Rest response using only for unsuccsessful operations as error
    order_creation_events: DashMap<
        ClientOrderId,
        (
            oneshot::Sender<CreationEventType>,
            Option<oneshot::Receiver<CreationEventType>>,
        ),
    >,

    order_cancellation_events: DashMap<
        ExchangeOrderId,
        (
            oneshot::Sender<CancelaltionEventType>,
            Option<oneshot::Receiver<CancelaltionEventType>>,
        ),
    >,
    application_manager: ApplicationManager,
    features: ExchangeFeatures,
}

impl Exchange {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        websocket_channels: Vec<String>,
        exchange_interaction: Box<dyn ExchangeClient>,
        features: ExchangeFeatures,
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
            order_creation_events: DashMap::new(),
            order_cancellation_events: DashMap::new(),
            // TODO in the future application_manager have to be passed as parameter
            application_manager: ApplicationManager::default(),
            features,
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
        if let Some((_, (tx, _))) = self.order_creation_events.remove(&client_order_id) {
            tx.send(CreateOrderResult::successed(exchange_order_id, source_type))
                .unwrap();
        }
    }

    fn raise_order_cancelled(
        &self,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        source_type: EventSourceType,
    ) {
        dbg!(&"DAAAAA");
        if let Some((_, (tx, _))) = self.order_cancellation_events.remove(&exchange_order_id) {
            tx.send(CancelOrderResult::successed(
                client_order_id,
                source_type,
                None,
            ))
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

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_interaction
            .set_order_created_callback(Box::new(
                move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade()
                {
                    Some(exchange) => exchange.raise_order_cancelled(
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
            "Websocket message from {}: {}",
            self.exchange_account_id, msg
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

        if let Some(rest_error) = self.get_rest_error_order(request_outcome, order) {
            return CreateOrderResult::failed(rest_error, EventSourceType::Rest);
        }

        let created_order_id = self.exchange_interaction.get_order_id(&request_outcome);
        CreateOrderResult::successed(created_order_id, EventSourceType::Rest)
    }

    fn get_rest_error(&self, response: &RestRequestOutcome) -> Option<ExchangeError> {
        self.get_rest_error_main(response, None, None)
    }

    fn get_rest_error_order(
        &self,
        response: &RestRequestOutcome,
        order: &OrderCreating,
    ) -> Option<ExchangeError> {
        let client_order_id = order.header.client_order_id.to_string();
        let exchange_account_id = order.header.exchange_account_id.to_string();
        let log_template = format!("order {} {}", client_order_id, exchange_account_id);
        let args_to_log = Some(vec![client_order_id, exchange_account_id]);

        self.get_rest_error_main(response, Some(log_template), args_to_log)
    }

    pub fn get_rest_error_main(
        &self,
        response: &RestRequestOutcome,
        log_template: Option<String>,
        args_to_log: Option<Vec<String>>,
    ) -> Option<ExchangeError> {
        let result_error = match response.status {
            StatusCode::UNAUTHORIZED => ExchangeError::new(
                ExchangeErrorType::Authentication,
                response.content.clone(),
                None,
            ),
            StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => ExchangeError::new(
                ExchangeErrorType::ServiceUnavailable,
                response.content.clone(),
                None,
            ),
            StatusCode::TOO_MANY_REQUESTS => {
                ExchangeError::new(ExchangeErrorType::RateLimit, response.content.clone(), None)
            }
            _ => match Self::check_content(&response.content) {
                CheckContent::Empty => {
                    if self.features.empty_response_is_ok {
                        return None;
                    }

                    ExchangeError::new(
                        ExchangeErrorType::Unknown,
                        "Empty response".to_owned(),
                        None,
                    )
                }
                CheckContent::Err(error) => error,
                CheckContent::Usable => {
                    if let Some(rest_error) =
                        self.exchange_interaction.is_rest_error_code(&response)
                    {
                        let error_type = self.exchange_interaction.get_error_type(&rest_error);

                        ExchangeError::new(error_type, rest_error.message, Some(rest_error.code))
                    } else {
                        return None;
                    }
                }
            },
        };

        let mut msg_to_log = format!(
            "Response has an error {:?}, on {}: {:?}",
            result_error.error_type, self.exchange_account_id, result_error
        );

        if args_to_log.is_some() {
            let args = args_to_log.unwrap();
            msg_to_log = format!(" {} with args: {:?}", msg_to_log, args);
        }

        if log_template.is_some() {
            let template = log_template.unwrap();
            msg_to_log = format!(" {}", template);
        }

        let log_level = match result_error.error_type {
            ExchangeErrorType::RateLimit
            | ExchangeErrorType::Authentication
            | ExchangeErrorType::InsufficientFunds
            | ExchangeErrorType::InvalidOrder => Level::Error,
            _ => Level::Warn,
        };

        log::log!(log_level, "{}. Response: {:?}", &msg_to_log, &response);

        // TODO some HandleRestError via BotBase

        Some(result_error)
    }

    fn check_content(content: &str) -> CheckContent {
        // TODO is that OK to deserialize it each time here?
        match serde_json::from_str::<Value>(&content) {
            Ok(data) => {
                match data {
                    Value::Null => return CheckContent::Empty,
                    Value::Array(array) => {
                        if array.is_empty() {
                            return CheckContent::Empty;
                        }
                    }
                    Value::Object(val) => {
                        if val.is_empty() {
                            return CheckContent::Empty;
                        }
                    }
                    Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                        return CheckContent::Usable
                    }
                };

                return CheckContent::Usable;
            }
            Err(_) => {
                return CheckContent::Err(ExchangeError::new(
                    ExchangeErrorType::Unknown,
                    "Unable to parse response".to_owned(),
                    None,
                ));
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

        self.order_creation_events
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

    pub async fn cancel_order(
        &self,
        order: &OrderCancelling,
        cancellation_token: CancellationToken,
    ) {
        self.exchange_interaction.cancel_order(&order).await;
        // FIXME HandleCancelORderSucceeded etc
    }

    pub async fn cancel_all_orders(&self, currency_pair: CurrencyPair) {
        self.exchange_interaction
            .cancel_all_orders(currency_pair)
            .await;
    }

    pub async fn get_account_info(&self) {
        self.exchange_interaction.get_account_info().await;
    }

    pub async fn get_open_orders(&self) -> anyhow::Result<Vec<OrderInfo>> {
        // Bugs on exchange server can lead to Err even if order was opened
        loop {
            match self.get_open_orders_impl().await {
                Ok(gotten_orders) => return Ok(gotten_orders),
                Err(error) => warn!("{}", error),
            }
        }
    }

    // Bugs on exchange server can lead to Err even if order was opened
    async fn get_open_orders_impl(&self) -> anyhow::Result<Vec<OrderInfo>> {
        let open_orders;
        match self.features.open_orders_type {
            OpenOrdersType::AllCurrencyPair => {
                // TODO implement in the future
                //reserve_when_acailable().await
                let response = self.exchange_interaction.request_open_orders().await;

                info!(
                    "get_open_orders() response on {}: {:?}",
                    self.exchange_account_id, response
                );

                if let Some(error) = self.get_rest_error(&response) {
                    bail!("Rest error appeared during request: {}", error.message)
                }

                open_orders = self.exchange_interaction.parse_open_orders(&response);

                return Ok(open_orders);
            }
            OpenOrdersType::OneCurrencyPair => {
                // TODO implement in the future
                //reserve_when_acailable().await
                // TODO other actions here have to be written after build_metadata() implementation

                return Err(anyhow!(""));
            }
            _ => bail!(
                "Unsupported open_orders_type: {:?}",
                self.features.open_orders_type
            ),
        }

        // TODO Prolly should to be moved in first and second branches in match above
        //if (add_missing_open_orders) {
        //    add_missing_open_orders(openOrders);
        //}
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
