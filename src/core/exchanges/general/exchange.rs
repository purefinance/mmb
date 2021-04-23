use super::commission::Commission;
use super::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::connectivity::connectivity_manager::GetWSParamsCallback;
use crate::core::exchanges::general::features::ExchangeFeatures;
use crate::core::exchanges::general::order::cancel::CancelOrderResult;
use crate::core::exchanges::general::order::create::CreateOrderResult;
use crate::core::orders::order::{OrderEventType, OrderHeader};
use crate::core::orders::pool::OrdersPool;
use crate::core::orders::{order::ExchangeOrderId, pool::OrderRef};
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole,
    exchanges::common::ExchangeAccountId,
    exchanges::{
        application_manager::ApplicationManager,
        common::CurrencyPair,
        common::{ExchangeError, ExchangeErrorType, RestRequestOutcome, SpecificCurrencyPair},
        traits::ExchangeClient,
    },
};
use crate::core::{
    connectivity::{connectivity_manager::ConnectivityManager, websocket_actor::WebSocketParams},
    orders::order::ClientOrderId,
};
use crate::core::{
    exchanges::common::{Amount, CurrencyCode, CurrencyId, Price},
    orders::event::OrderEvent,
};
use anyhow::{bail, Context, Error, Result};
use awc::http::StatusCode;
use dashmap::DashMap;
use futures::Future;
use log::{info, warn, Level};
use parking_lot::Mutex;
use serde_json::Value;
use std::pin::Pin;
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestResult<T> {
    Success(T),
    Error(ExchangeError),
    // TODO for that we need match binance_error_code as number with ExchangeErrorType
    //Error(ExchangeErrorType),
}

enum CheckContent {
    Empty,
    Err(ExchangeError),
    Usable,
}

pub(crate) struct PriceLevel {
    pub price: Price,
    pub amount: Amount,
}

pub(crate) struct OrderBookTop {
    pub ask: Option<PriceLevel>,
    pub bid: Option<PriceLevel>,
}

pub struct Exchange {
    pub exchange_account_id: ExchangeAccountId,
    websocket_host: String,
    specific_currency_pairs: Vec<SpecificCurrencyPair>,
    websocket_channels: Vec<String>,
    pub(super) exchange_client: Box<dyn ExchangeClient>,
    pub orders: Arc<OrdersPool>,
    connectivity_manager: Arc<ConnectivityManager>,

    // It allows to send and receive notification about event in websocket channel
    // Websocket event is main source detecting order creation result
    // Rest response using only for unsuccessful operations as error
    pub(super) order_creation_events: DashMap<
        ClientOrderId,
        (
            oneshot::Sender<CreateOrderResult>,
            Option<oneshot::Receiver<CreateOrderResult>>,
        ),
    >,

    pub(super) order_cancellation_events: DashMap<
        ExchangeOrderId,
        (
            oneshot::Sender<CancelOrderResult>,
            Option<oneshot::Receiver<CancelOrderResult>>,
        ),
    >,
    pub(super) features: ExchangeFeatures,
    pub(super) event_channel: Mutex<mpsc::Sender<OrderEvent>>,
    application_manager: ApplicationManager,
    pub(super) commission: Commission,
    pub(super) supported_currencies: DashMap<CurrencyCode, CurrencyId>,
    pub(super) supported_symbols: Mutex<Vec<Arc<CurrencyPairMetadata>>>,
    pub(super) symbols: DashMap<CurrencyPair, Arc<CurrencyPairMetadata>>,
    pub(super) currencies: Mutex<Vec<CurrencyCode>>,
    pub(crate) order_book_top: DashMap<CurrencyPair, OrderBookTop>,
}

pub type BoxExchangeClient = Box<dyn ExchangeClient + Send + Sync + 'static>;

impl Exchange {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        websocket_host: String,
        specific_currency_pairs: Vec<SpecificCurrencyPair>,
        websocket_channels: Vec<String>,
        exchange_client: BoxExchangeClient,
        features: ExchangeFeatures,
        event_channel: mpsc::Sender<OrderEvent>,
        commission: Commission,
    ) -> Arc<Self> {
        let connectivity_manager = ConnectivityManager::new(exchange_account_id.clone());

        let exchange = Arc::new(Self {
            exchange_account_id: exchange_account_id.clone(),
            websocket_host,
            specific_currency_pairs,
            websocket_channels,
            exchange_client,
            orders: OrdersPool::new(),
            connectivity_manager,
            order_creation_events: DashMap::new(),
            order_cancellation_events: DashMap::new(),
            supported_currencies: Default::default(),
            supported_symbols: Default::default(),
            // TODO in the future application_manager have to be passed as parameter
            application_manager: ApplicationManager::default(),
            features,
            event_channel: Mutex::new(event_channel),
            commission,
            symbols: Default::default(),
            currencies: Default::default(),
            order_book_top: Default::default(),
        });

        exchange.clone().setup_connectivity_manager();
        exchange.clone().setup_exchange_client();

        exchange
    }

    fn setup_connectivity_manager(self: Arc<Self>) {
        let exchange_weak = Arc::downgrade(&self);
        self.connectivity_manager
            .set_callback_msg_received(Box::new(move |data| match exchange_weak.upgrade() {
                Some(exchange) => exchange.on_websocket_message(data),
                None => info!(
                    "Unable to upgrade weak reference to Exchange instance. Probably it's dead"
                ),
            }));
    }

    fn setup_exchange_client(self: Arc<Self>) {
        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_order_created_callback(Box::new(
            move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade() {
                Some(exchange) => {
                    exchange.raise_order_created(client_order_id, exchange_order_id, source_type)
                }
                None => info!(
                    "Unable to upgrade weak reference to Exchange instance. Probably it's dead",
                ),
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_order_cancelled_callback(Box::new(
            move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade() {
                Some(exchange) => {
                    // FIXME How to handle Result here?
                    exchange.raise_order_cancelled(client_order_id, exchange_order_id, source_type);
                }
                None => info!(
                    "Unable to upgrade weak reference to Exchange instance. Probably it's dead",
                ),
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client
            .set_handle_order_filled_callback(Box::new(move |event_data| {
                match exchange_weak.upgrade() {
                    Some(exchange) => {
                        // FIXME How to handle Result here?
                        exchange.handle_order_filled(event_data);
                    }
                    None => info!(
                        "Unable to upgrade weak referene to Exchange instance. Probably it's dead",
                    ),
                }
            }));
    }

    fn on_websocket_message(&self, msg: &str) {
        if self
            .application_manager
            .cancellation_token
            .check_cancellation_requested()
        {
            return;
        }

        if self.exchange_client.should_log_message(msg) {
            self.log_websocket_message(msg);
        }

        let callback_outcome = self.exchange_client.on_websocket_message(msg);
        if let Err(error) = callback_outcome {
            warn!(
                "Error occurred while websocket message processing: {}",
                error
            );
        }
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
            let exchange = exchange_weak
                .upgrade()
                .expect("Unable to upgrade reference to Exchange");
            let params = exchange.get_websocket_params(websocket_role);
            Box::pin(params) as Pin<Box<dyn Future<Output = Result<WebSocketParams>>>>
        }) as GetWSParamsCallback;

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

    pub(super) fn get_rest_error(&self, response: &RestRequestOutcome) -> Option<ExchangeError> {
        self.get_rest_error_main(response, None, None)
    }

    pub(super) fn get_rest_error_order(
        &self,
        response: &RestRequestOutcome,
        order_header: &OrderHeader,
    ) -> Option<ExchangeError> {
        let client_order_id = order_header.client_order_id.to_string();
        let exchange_account_id = order_header.exchange_account_id.to_string();
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
                CheckContent::Usable => match self.exchange_client.is_rest_error_code(&response) {
                    Ok(_) => return None,
                    Err(mut error) => match error.error_type {
                        ExchangeErrorType::ParsingError => error,
                        _ => {
                            self.exchange_client.clarify_error_type(&mut error);
                            error
                        }
                    },
                },
            },
        };

        let mut msg_to_log = format!(
            "Response has an error {:?}, on {}: {:?}",
            result_error.error_type, self.exchange_account_id, result_error
        );

        if let Some(args) = args_to_log {
            msg_to_log = format!(" {} with args: {:?}", msg_to_log, args);
        }

        if let Some(template) = log_template {
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
        return match serde_json::from_str::<Value>(&content) {
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

                CheckContent::Usable
            }
            Err(_) => CheckContent::Err(ExchangeError::new(
                ExchangeErrorType::Unknown,
                "Unable to parse response".to_owned(),
                None,
            )),
        };
    }

    pub async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> anyhow::Result<()> {
        self.exchange_client
            .cancel_all_orders(currency_pair)
            .await?;

        Ok(())
    }

    pub(super) fn handle_parse_error(
        &self,
        error: Error,
        response: RestRequestOutcome,
        log_template: String,
        args_to_log: Option<Vec<String>>,
    ) -> anyhow::Result<()> {
        let content = response.content;
        let log_event_level = match serde_json::from_str::<Value>(&content) {
            Ok(_) => Level::Error,
            Err(_) => Level::Warn,
        };

        let mut msg_to_log = format!(
            "Error parsing response {}, on {}: {}. Error: {}",
            log_template,
            self.exchange_account_id,
            content,
            error.to_string()
        );

        if let Some(args) = args_to_log {
            msg_to_log = format!(" {} with args: {:?}", msg_to_log, args);
        }

        // TODO Add some other fields as Exchange::Id, Exchange::Name
        log::log!(log_event_level, "{}.", msg_to_log,);

        if log_event_level == Level::Error {
            bail!("{}", msg_to_log);
        }

        Ok(())
    }

    pub async fn get_websocket_params(
        self: Arc<Self>,
        websocket_role: WebSocketRole,
    ) -> Result<WebSocketParams> {
        let ws_path = match websocket_role {
            WebSocketRole::Main => {
                // TODO remove hardcode or probably extract to common_interaction trait
                self.exchange_client.build_ws_main_path(
                    &self.specific_currency_pairs[..],
                    &self.websocket_channels[..],
                )
            }
            WebSocketRole::Secondary => self.exchange_client.build_ws_secondary_path().await?,
        };

        Ok(self.create_websocket_params(&ws_path))
    }

    pub(crate) fn add_event_on_order_change(
        &self,
        order_ref: &OrderRef,
        event_type: OrderEventType,
    ) -> Result<()> {
        if event_type == OrderEventType::CancelOrderSucceeded {
            order_ref.fn_mut(|order| order.internal_props.was_cancellation_event_raised = true)
        }

        if order_ref.is_finished() {
            let _ = self
                .orders
                .not_finished
                .remove(&order_ref.client_order_id());
        }

        let order_event = OrderEvent::new(order_ref.clone(), order_ref.status(), event_type, None);
        self.event_channel
            .lock()
            .send(order_event)
            .context("Unable to send event. Probably receiver is already dead")?;

        Ok(())
    }
}
