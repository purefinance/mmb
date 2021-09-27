use std::sync::Arc;

use anyhow::{bail, Context, Error, Result};
use awc::http::StatusCode;
use dashmap::DashMap;
use futures::FutureExt;
use itertools::Itertools;
use log::{error, info, warn, Level};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::sync::{broadcast, oneshot};

use super::commission::Commission;
use super::currency_pair_metadata::CurrencyPairMetadata;
use crate::core::connectivity::connectivity_manager::GetWSParamsCallback;
use crate::core::exchanges::events::ExchangeEvent;
use crate::core::exchanges::general::features::ExchangeFeatures;
use crate::core::exchanges::general::order::cancel::CancelOrderResult;
use crate::core::exchanges::general::order::create::CreateOrderResult;
use crate::core::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::core::orders::event::OrderEventType;
use crate::core::orders::order::{OrderHeader, OrderSide};
use crate::core::orders::pool::OrdersPool;
use crate::core::orders::{order::ExchangeOrderId, pool::OrderRef};
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole,
    exchanges::common::ExchangeAccountId,
    exchanges::{
        common::CurrencyPair,
        common::{ExchangeError, ExchangeErrorType, RestRequestOutcome},
        traits::ExchangeClient,
    },
    lifecycle::application_manager::ApplicationManager,
    lifecycle::cancellation_token::CancellationToken,
};

use crate::core::{
    connectivity::{connectivity_manager::ConnectivityManager, websocket_actor::WebSocketParams},
    orders::order::ClientOrderId,
};
use crate::core::{
    exchanges::common::{Amount, CurrencyCode, Price},
    orders::event::OrderEvent,
};
use std::fmt::{Arguments, Debug, Write};

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestResult<T> {
    Success(T),
    Error(ExchangeError),
    // TODO for that we need match binance_error_code as number with ExchangeErrorType
    //Error(ExchangeErrorType),
}

impl<T> RequestResult<T> {
    pub fn get_error(&self) -> Option<ExchangeError> {
        match self {
            RequestResult::Success(_) => None,
            RequestResult::Error(exchange_error) => Some(exchange_error.clone()),
        }
    }
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
    pub(super) events_channel: broadcast::Sender<ExchangeEvent>,
    pub(super) application_manager: Arc<ApplicationManager>,
    pub(crate) timeout_manager: Arc<TimeoutManager>,
    pub(super) commission: Commission,
    pub(super) supported_symbols: Mutex<Vec<Arc<CurrencyPairMetadata>>>,
    pub(super) symbols: DashMap<CurrencyPair, Arc<CurrencyPairMetadata>>,
    pub(crate) currencies: Mutex<Vec<CurrencyCode>>,
    pub(crate) order_book_top: DashMap<CurrencyPair, OrderBookTop>,
    pub(super) wait_cancel_order: DashMap<ClientOrderId, broadcast::Sender<()>>,
    pub(super) orders_finish_events: DashMap<ClientOrderId, oneshot::Sender<()>>,
    pub(super) orders_created_events: DashMap<ClientOrderId, oneshot::Sender<()>>,
    pub(crate) leverage_by_currency_pair: DashMap<CurrencyPair, Decimal>,
}

pub type BoxExchangeClient = Box<dyn ExchangeClient + Send + Sync + 'static>;

impl Exchange {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        exchange_client: BoxExchangeClient,
        features: ExchangeFeatures,
        events_channel: broadcast::Sender<ExchangeEvent>,
        application_manager: Arc<ApplicationManager>,
        timeout_manager: Arc<TimeoutManager>,
        commission: Commission,
    ) -> Arc<Self> {
        let connectivity_manager = ConnectivityManager::new(exchange_account_id.clone());

        let exchange = Arc::new(Self {
            exchange_account_id: exchange_account_id.clone(),
            exchange_client,
            orders: OrdersPool::new(),
            connectivity_manager,
            order_creation_events: DashMap::new(),
            order_cancellation_events: DashMap::new(),
            supported_symbols: Default::default(),
            application_manager,
            features,
            events_channel,
            timeout_manager,
            commission,
            symbols: Default::default(),
            currencies: Default::default(),
            order_book_top: Default::default(),
            wait_cancel_order: DashMap::new(),
            orders_finish_events: DashMap::new(),
            orders_created_events: DashMap::new(),
            leverage_by_currency_pair: DashMap::new(),
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
                None => info!("Unable to upgrade weak reference to Exchange instance"),
            }));
    }

    fn setup_exchange_client(self: Arc<Self>) {
        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_order_created_callback(Box::new(
            move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade() {
                Some(exchange) => {
                    exchange.raise_order_created(&client_order_id, &exchange_order_id, source_type)
                }
                None => info!("Unable to upgrade weak reference to Exchange instance",),
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_order_cancelled_callback(Box::new(
            move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade() {
                Some(exchange) => {
                    let raise_outcome = exchange.raise_order_cancelled(
                        client_order_id,
                        exchange_order_id,
                        source_type,
                    );

                    if let Err(error) = raise_outcome {
                        let error_message = format!("Error in raise_order_cancelled: {:?}", error);
                        error!("{}", error_message);
                        exchange
                            .application_manager
                            .clone()
                            .spawn_graceful_shutdown(error_message);
                    };
                }
                None => info!("Unable to upgrade weak reference to Exchange instance",),
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client
            .set_handle_order_filled_callback(Box::new(move |event_data| {
                match exchange_weak.upgrade() {
                    Some(exchange) => {
                        let handle_outcome = exchange.handle_order_filled(event_data);

                        if let Err(error) = handle_outcome {
                            let error_message =
                                format!("Error in handle_order_filled: {:?}", error);
                            error!("{}", error_message);
                            exchange
                                .application_manager
                                .clone()
                                .spawn_graceful_shutdown(error_message);
                        };
                    }
                    None => info!("Unable to upgrade weak reference to Exchange instance",),
                }
            }));
    }

    fn on_websocket_message(&self, msg: &str) {
        if self
            .application_manager
            .stop_token()
            .is_cancellation_requested()
        {
            return;
        }

        if self.exchange_client.should_log_message(msg) {
            self.log_websocket_message(msg);
        }

        let callback_outcome = self.exchange_client.on_websocket_message(msg);
        if let Err(error) = callback_outcome {
            warn!(
                "Error occurred while websocket message processing: {:?}",
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
        let get_websocket_params: GetWSParamsCallback = Box::new(move |websocket_role| {
            exchange_weak
                .upgrade()
                .expect("Unable to upgrade reference to Exchange")
                .get_websocket_params(websocket_role)
                .boxed()
        });

        let is_enabled_secondary_websocket = self
            .exchange_client
            .is_websocket_enabled(WebSocketRole::Secondary);

        let is_connected = self
            .connectivity_manager
            .clone()
            .connect(is_enabled_secondary_websocket, get_websocket_params)
            .await;

        if !is_connected {
            // TODO finish_connected
        }
        // TODO all other logs and finish_connected
    }

    pub(crate) fn get_rest_error(&self, response: &RestRequestOutcome) -> Option<ExchangeError> {
        self.get_rest_error_main(response, format_args!(""))
    }

    pub(super) fn get_rest_error_order(
        &self,
        response: &RestRequestOutcome,
        order_header: &OrderHeader,
    ) -> Option<ExchangeError> {
        let client_order_id = &order_header.client_order_id;
        let exchange_account_id = &order_header.exchange_account_id;
        self.get_rest_error_main(
            response,
            format_args!("order {} {}", client_order_id, exchange_account_id),
        )
    }

    pub fn get_rest_error_main(
        &self,
        response: &RestRequestOutcome,
        log_template: Arguments,
    ) -> Option<ExchangeError> {
        use ExchangeErrorType::*;

        let error = match response.status {
            StatusCode::UNAUTHORIZED => {
                ExchangeError::new(Authentication, response.content.clone(), None)
            }
            StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => {
                ExchangeError::new(ServiceUnavailable, response.content.clone(), None)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                ExchangeError::new(RateLimit, response.content.clone(), None)
            }
            _ => match Self::check_content(&response.content) {
                CheckContent::Empty => {
                    if self.features.empty_response_is_ok {
                        return None;
                    }

                    ExchangeError::new(Unknown, "Empty response".to_owned(), None)
                }
                CheckContent::Err(error) => error,
                CheckContent::Usable => match self.exchange_client.is_rest_error_code(response) {
                    Ok(_) => return None,
                    Err(mut error) => match error.error_type {
                        ParsingError => error,
                        _ => {
                            // TODO For Aax Pending time should be received inside clarify_error_type
                            self.exchange_client.clarify_error_type(&mut error);
                            error
                        }
                    },
                },
            },
        };

        let extra_data_len = 512; // just apriori estimation
        let mut msg = String::with_capacity(error.message.len() + extra_data_len);
        write!(
            &mut msg,
            "Response has an error {:?}, on {}: {:?}",
            error.error_type, self.exchange_account_id, error
        )
        .expect("Writing rest error");

        write!(&mut msg, " {}", log_template).expect("Writing rest error");

        let log_level = match error.error_type {
            RateLimit | Authentication | InsufficientFunds | InvalidOrder => Level::Error,
            _ => Level::Warn,
        };

        log::log!(log_level, "{}. Response: {:?}", &msg, response);

        // TODO some HandleRestError via BotBase

        Some(error)
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
        response: &RestRequestOutcome,
        log_template: String,
        args_to_log: Option<Vec<String>>,
    ) -> anyhow::Result<()> {
        let content = &response.content;
        let log_event_level = match serde_json::from_str::<Value>(content) {
            Ok(_) => Level::Error,
            Err(_) => Level::Warn,
        };

        let mut msg_to_log = format!(
            "Error parsing response {}, on {}: {}. Error: {:?}",
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
        role: WebSocketRole,
    ) -> Result<WebSocketParams> {
        let ws_url = self.exchange_client.create_ws_url(role).await?;
        Ok(WebSocketParams::new(ws_url))
    }

    pub(crate) fn add_event_on_order_change(
        &self,
        order_ref: &OrderRef,
        event_type: OrderEventType,
    ) -> Result<()> {
        if let OrderEventType::CancelOrderSucceeded = event_type {
            order_ref.fn_mut(|order| order.internal_props.was_cancellation_event_raised = true)
        }

        if order_ref.is_finished() {
            let _ = self
                .orders
                .not_finished
                .remove(&order_ref.client_order_id());
        }

        let event = ExchangeEvent::OrderEvent(OrderEvent::new(order_ref.clone(), event_type));
        self.events_channel
            .send(event)
            .context("Unable to send event. Probably receiver is already dropped")?;

        Ok(())
    }

    pub async fn cancel_opened_orders(
        self: Arc<Self>,
        cancellation_token: CancellationToken,
        add_missing_open_orders: bool,
    ) {
        match self.get_open_orders(add_missing_open_orders).await {
            Err(error) => {
                error!(
                    "Unable to get opened order for exchange account id {}: {:?}",
                    self.exchange_account_id, error,
                );
            }
            Ok(orders) => {
                tokio::select! {
                    _ = self.cancel_orders(orders.clone(), cancellation_token.clone()) => {
                        ()
                    },
                    _ = cancellation_token.when_cancelled() => {
                        log::error!(
                            "Opened orders canceling for exchange account id {} was interrupted by CancellationToken for list of orders {:?}",
                            self.exchange_account_id,
                            orders
                                .iter()
                                .map(|x| x.client_order_id.as_str())
                                .collect_vec(),
                        );
                        ()
                    },
                }
            }
        }
    }

    pub fn get_balance_reservation_currency_code(
        &self,
        currency_pair_metadata: Arc<CurrencyPairMetadata>,
        side: OrderSide,
    ) -> CurrencyCode {
        self.exchange_client
            .get_balance_reservation_currency_code(currency_pair_metadata, side)
    }
}
