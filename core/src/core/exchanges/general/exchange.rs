use std::sync::{Arc, Weak};

use anyhow::{bail, Context, Error, Result};
use awc::http::StatusCode;
use dashmap::DashMap;
use futures::FutureExt;
use itertools::Itertools;
use log::log;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::traits_ext::send_expected::SendExpectedByRef;
use mmb_utils::{nothing_to_do, DateTime};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::sync::{broadcast, oneshot};

use super::commission::Commission;
use super::polling_timeout_manager::PollingTimeoutManager;
use super::symbol::Symbol;
use crate::core::connectivity::connectivity_manager::GetWSParamsCallback;
#[cfg(debug_assertions)]
use crate::core::exchanges::common::SpecificCurrencyPair;
use crate::core::exchanges::common::{ActivePosition, ClosedPosition, TradePlace};
use crate::core::exchanges::events::{
    BalanceUpdateEvent, ExchangeBalance, ExchangeBalancesAndPositions, ExchangeEvent,
    LiquidationPriceEvent, Trade,
};
use crate::core::exchanges::general::features::{BalancePositionOption, ExchangeFeatures};
use crate::core::exchanges::general::order::cancel::CancelOrderResult;
use crate::core::exchanges::general::order::create::CreateOrderResult;
use crate::core::exchanges::general::request_type::RequestType;
use crate::core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use crate::core::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::core::misc::derivative_position::DerivativePosition;
use crate::core::misc::time::time_manager;
use crate::core::orders::buffered_fills::buffered_canceled_orders_manager::BufferedCanceledOrdersManager;
use crate::core::orders::buffered_fills::buffered_fills_manager::BufferedFillsManager;
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
};

use crate::core::balance_manager::balance_manager::BalanceManager;
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
    pub symbols: DashMap<CurrencyPair, Arc<Symbol>>,
    /// Actualised orders data for active order and some late cached orders
    pub orders: Arc<OrdersPool>,
    pub(crate) currencies: Mutex<Vec<CurrencyCode>>,
    pub(crate) leverage_by_currency_pair: DashMap<CurrencyPair, Decimal>,
    pub(crate) order_book_top: DashMap<CurrencyPair, OrderBookTop>,
    pub(super) exchange_client: Box<dyn ExchangeClient>,
    pub(super) features: ExchangeFeatures,
    pub(super) events_channel: broadcast::Sender<ExchangeEvent>,
    pub(super) application_manager: Arc<ApplicationManager>,
    pub(super) commission: Commission,
    pub(super) wait_cancel_order: DashMap<ClientOrderId, broadcast::Sender<()>>,
    pub(super) wait_finish_order: DashMap<ClientOrderId, broadcast::Sender<OrderRef>>,
    pub(super) polling_trades_counts: DashMap<ExchangeAccountId, u32>,
    pub(super) polling_timeout_manager: PollingTimeoutManager,
    pub(super) orders_finish_events: DashMap<ClientOrderId, oneshot::Sender<()>>,
    pub(super) orders_created_events: DashMap<ClientOrderId, oneshot::Sender<()>>,
    pub(super) last_trades_update_time: DashMap<TradePlace, DateTime>,
    pub(super) last_trades: DashMap<TradePlace, Trade>,
    pub(super) timeout_manager: Arc<TimeoutManager>,
    pub(super) balance_manager: Mutex<Option<Weak<Mutex<BalanceManager>>>>,
    pub(super) buffered_fills_manager: Mutex<BufferedFillsManager>,
    pub(super) buffered_canceled_orders_manager: Mutex<BufferedCanceledOrdersManager>,
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
    connectivity_manager: Arc<ConnectivityManager>,
}

pub type BoxExchangeClient = Box<dyn ExchangeClient + Send + Sync + 'static>;

impl Exchange {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        exchange_client: BoxExchangeClient,
        features: ExchangeFeatures,
        timeout_arguments: RequestTimeoutArguments,
        events_channel: broadcast::Sender<ExchangeEvent>,
        application_manager: Arc<ApplicationManager>,
        timeout_manager: Arc<TimeoutManager>,
        commission: Commission,
    ) -> Arc<Self> {
        let connectivity_manager = ConnectivityManager::new(exchange_account_id);
        let polling_timeout_manager = PollingTimeoutManager::new(timeout_arguments);

        let exchange = Arc::new(Self {
            exchange_account_id,
            exchange_client,
            orders: OrdersPool::new(),
            connectivity_manager,
            order_creation_events: DashMap::new(),
            order_cancellation_events: DashMap::new(),
            application_manager,
            features,
            events_channel,
            timeout_manager,
            commission,
            symbols: Default::default(),
            currencies: Default::default(),
            order_book_top: Default::default(),
            wait_cancel_order: DashMap::new(),
            wait_finish_order: DashMap::new(),
            polling_trades_counts: DashMap::new(),
            polling_timeout_manager,
            orders_finish_events: DashMap::new(),
            orders_created_events: DashMap::new(),
            leverage_by_currency_pair: DashMap::new(),
            last_trades_update_time: DashMap::new(),
            last_trades: DashMap::new(),
            balance_manager: Mutex::new(None),
            buffered_fills_manager: Mutex::new(BufferedFillsManager::new()),
            buffered_canceled_orders_manager: Mutex::new(BufferedCanceledOrdersManager::new()),
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
                None => log::info!("Unable to upgrade weak reference to Exchange instance"),
            }));

        let exchange_weak = Arc::downgrade(&self);
        self.connectivity_manager
            .set_callback_connecting(Box::new(move || match exchange_weak.upgrade() {
                Some(exchange) => exchange.on_connecting(),
                None => log::info!("Unable to upgrade weak reference to Exchange instance"),
            }));
    }

    fn setup_exchange_client(self: Arc<Self>) {
        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_order_created_callback(Box::new(
            move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade() {
                Some(exchange) => {
                    exchange.raise_order_created(&client_order_id, &exchange_order_id, source_type)
                }
                None => log::info!("Unable to upgrade weak reference to Exchange instance",),
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
                        log::error!("{}", error_message);
                        exchange
                            .application_manager
                            .clone()
                            .spawn_graceful_shutdown(error_message);
                    };
                }
                None => log::info!("Unable to upgrade weak reference to Exchange instance",),
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
                            log::error!("{}", error_message);
                            exchange
                                .application_manager
                                .clone()
                                .spawn_graceful_shutdown(error_message);
                        };
                    }
                    None => log::info!("Unable to upgrade weak reference to Exchange instance",),
                }
            }));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_handle_trade_callback(Box::new(
            move |currency_pair, trade_id, price, quantity, order_side, transaction_time| {
                match exchange_weak.upgrade() {
                    Some(exchange) => {
                        let handle_outcome = exchange.handle_trade(
                            currency_pair,
                            trade_id,
                            price,
                            quantity,
                            order_side,
                            transaction_time,
                        );

                        if let Err(error) = handle_outcome {
                            let error_message = format!("Error in handle_trade: {:?}", error);
                            log::error!("{}", error_message);
                            exchange
                                .application_manager
                                .clone()
                                .spawn_graceful_shutdown(error_message);
                        };
                    }
                    None => log::info!("Unable to upgrade weak reference to Exchange instance",),
                }
            },
        ));
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
            log::warn!(
                "Error occurred while websocket message processing: {:?}",
                error
            );
        }
    }

    fn on_connecting(&self) {
        if self
            .application_manager
            .stop_token()
            .is_cancellation_requested()
        {
            return;
        }

        let callback_outcome = self.exchange_client.on_connecting();
        if let Err(error) = callback_outcome {
            log::warn!(
                "Error occurred while websocket message processing: {:?}",
                error
            );
        }
    }

    fn log_websocket_message(&self, msg: &str) {
        log::info!(
            "Websocket message from {}: {}",
            self.exchange_account_id,
            msg
        );
    }

    pub fn setup_balance_manager(&self, balance_manager: Arc<Mutex<BalanceManager>>) {
        *self.balance_manager.lock() = Some(Arc::downgrade(&balance_manager));
    }

    pub async fn connect(self: Arc<Self>) {
        self.try_connect().await;
        // TODO Reconnect
    }

    async fn try_connect(self: Arc<Self>) {
        // TODO IsWebSocketConnecting()
        log::info!("Websocket: Connecting on {}", "test_exchange_id");

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
            RateLimit | Authentication | InsufficientFunds | InvalidOrder => log::Level::Error,
            _ => log::Level::Warn,
        };

        log!(log_level, "{}. Response: {:?}", &msg, response);

        // TODO some HandleRestError via BotBase

        Some(error)
    }

    fn check_content(content: &str) -> CheckContent {
        if content.is_empty() {
            CheckContent::Empty
        } else {
            CheckContent::Usable
        }
    }

    pub async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()> {
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
    ) -> Result<()> {
        let content = &response.content;
        let log_event_level = match serde_json::from_str::<Value>(content) {
            Ok(_) => log::Level::Error,
            Err(_) => log::Level::Warn,
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
        log!(log_event_level, "{}.", msg_to_log,);

        if log_event_level == log::Level::Error {
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
                log::error!(
                    "Unable to get opened order for exchange account id {}: {:?}",
                    self.exchange_account_id,
                    error,
                );
            }
            Ok(orders) => {
                tokio::select! {
                    _ = self.cancel_orders(orders.clone(), cancellation_token.clone()) => nothing_to_do(),
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
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> CurrencyCode {
        self.exchange_client
            .get_balance_reservation_currency_code(symbol, side)
    }

    pub async fn close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<ClosedPosition> {
        let response = self
            .exchange_client
            .request_close_position(position, price)
            .await
            .expect("request_close_position failed.");

        log::info!(
            "Close position response for {:?} {:?} {:?}",
            position,
            price,
            response,
        );

        self.exchange_client.is_rest_error_code(&response)?;

        self.exchange_client.parse_close_position(&response)
    }

    pub async fn close_position_loop(
        &self,
        position: &ActivePosition,
        price: Option<Decimal>,
        cancellation_token: CancellationToken,
    ) -> ClosedPosition {
        log::info!("Closing position {}", position.id);

        loop {
            self.timeout_manager
                .reserve_when_available(
                    self.exchange_account_id,
                    RequestType::GetActivePositions,
                    None,
                    cancellation_token.clone(),
                )
                .expect("Failed to reserve timeout_manager for close_position")
                .await;

            log::info!("Closing position request reserved {}", position.id);

            if let Ok(closed_position) = self.close_position(position, price).await {
                log::info!("Closed position {}", position.id);
                return closed_position;
            }
        }
    }

    pub async fn get_active_positions(
        &self,
        cancellation_token: CancellationToken,
    ) -> Vec<ActivePosition> {
        loop {
            self.timeout_manager
                .reserve_when_available(
                    self.exchange_account_id,
                    RequestType::GetActivePositions,
                    None,
                    cancellation_token.clone(),
                )
                .expect("Failed to reserve timeout_manager for get_active_positions")
                .await;

            if let Ok(positions) = self.get_active_positions_by_features().await {
                return positions;
            }
        }
    }

    pub async fn get_active_positions_by_features(&self) -> Result<Vec<ActivePosition>> {
        match self.features.balance_position_option {
            BalancePositionOption::IndividualRequests => self.get_active_positions_core().await,
            BalancePositionOption::SingleRequest => {
                let result = self.get_balance_and_positions_core().await?;
                Ok(result
                    .positions
                    .context("Positions is none.")?
                    .into_iter()
                    .map(|x| ActivePosition::new(x))
                    .collect_vec())
            }
            BalancePositionOption::NonDerivative => {
                // TODO Should be implemented manually closing positions for non-derivative exchanges
                Ok(Vec::new())
            }
        }
    }

    async fn get_active_positions_core(&self) -> Result<Vec<ActivePosition>> {
        let response = self
            .exchange_client
            .request_get_position()
            .await
            .expect("request_close_position failed.");

        log::info!(
            "get_positions response on {:?} {:?}",
            self.exchange_account_id,
            response,
        );

        self.exchange_client.is_rest_error_code(&response)?;

        Ok(self.exchange_client.parse_get_position(&response))
    }

    pub(super) async fn get_balance_core(&self) -> Result<ExchangeBalancesAndPositions> {
        let response = self.exchange_client.request_get_balance().await?;

        log::info!(
            "get_balance_core response on {:?} {:?}",
            self.exchange_account_id,
            response,
        );

        self.exchange_client.is_rest_error_code(&response)?;

        Ok(self.exchange_client.parse_get_balance(&response))
    }

    async fn get_balance_and_positions(
        &self,
        cancellation_token: CancellationToken,
    ) -> Result<ExchangeBalancesAndPositions> {
        self.timeout_manager
            .reserve_when_available(
                self.exchange_account_id,
                RequestType::GetBalance,
                None,
                cancellation_token.clone(),
            )?
            .await;

        let balance_result = match self.features.balance_position_option {
            BalancePositionOption::NonDerivative => return self.get_balance_core().await,
            BalancePositionOption::SingleRequest => self.get_balance_and_positions_core().await?,
            BalancePositionOption::IndividualRequests => {
                let balances_result = self.get_balance_core().await?;

                if balances_result.positions.is_some() {
                    bail!("Exchange supports SingleRequest but Individual is used")
                }

                self.timeout_manager
                    .reserve_when_available(
                        self.exchange_account_id,
                        RequestType::GetActivePositions,
                        None,
                        cancellation_token.clone(),
                    )?
                    .await;

                let position_result = self.get_active_positions_core().await?;

                let balances = balances_result.balances;
                let positions = position_result
                    .into_iter()
                    .map(|x| x.derivative)
                    .collect_vec();

                ExchangeBalancesAndPositions {
                    balances,
                    positions: Some(positions),
                }
            }
        };

        if let Some(positions) = &balance_result.positions {
            for position in positions {
                if let Some(mut leverage) = self
                    .leverage_by_currency_pair
                    .get_mut(&position.currency_pair)
                {
                    *leverage.value_mut() = position.leverage;
                }
            }
        }

        Ok(balance_result)
    }

    async fn get_balance_and_positions_core(&self) -> Result<ExchangeBalancesAndPositions> {
        let response = self
            .exchange_client
            .request_get_balance_and_position()
            .await
            .expect("request_close_position failed.");

        log::info!(
            "get_balance_and_positions_core response on {:?} {:?}",
            self.exchange_account_id,
            response,
        );

        self.exchange_client.is_rest_error_code(&response)?;

        Ok(self.exchange_client.parse_get_balance(&response))
    }

    /// Remove currency pairs that aren't supported by the current exchange
    /// if all currencies aren't supported return None
    fn remove_unknown_currency_pairs(
        &self,
        positions: Option<Vec<DerivativePosition>>,
        balances: Vec<ExchangeBalance>,
    ) -> ExchangeBalancesAndPositions {
        let positions = positions.map(|x| {
            x.into_iter()
                .filter(|y| self.symbols.contains_key(&y.currency_pair))
                .collect_vec()
        });

        ExchangeBalancesAndPositions {
            balances,
            positions,
        }
    }

    fn handle_balances_and_positions(
        &self,
        balances_and_positions: ExchangeBalancesAndPositions,
    ) -> ExchangeBalancesAndPositions {
        self.events_channel
            .send_expected(ExchangeEvent::BalanceUpdate(BalanceUpdateEvent {
                exchange_account_id: self.exchange_account_id,
                balances_and_positions: balances_and_positions.clone(),
            }));

        if let Some(positions) = &balances_and_positions.positions {
            for position_info in positions {
                self.handle_liquidation_price(
                    position_info.currency_pair,
                    position_info.liquidation_price,
                    position_info.average_entry_price,
                    position_info.side.expect("position_info.side is None"),
                )
            }
        }

        balances_and_positions
    }

    pub async fn get_balance(
        &self,
        cancellation_token: CancellationToken,
    ) -> Option<ExchangeBalancesAndPositions> {
        let print_warn = |retry_attempt: i32, error: String| {
            log::warn!(
                "Failed to get balance for {} on retry {}: {}",
                self.exchange_account_id,
                retry_attempt,
                error
            )
        };

        for retry_attempt in 1..=5 {
            let balances_and_positions = self
                .get_balance_and_positions(cancellation_token.clone())
                .await;

            match balances_and_positions {
                Ok(ExchangeBalancesAndPositions {
                    positions,
                    balances,
                }) => {
                    if balances.is_empty() {
                        (print_warn)(retry_attempt, "balances is empty".into());
                        continue;
                    }

                    return Some(self.handle_balances_and_positions(
                        self.remove_unknown_currency_pairs(positions, balances),
                    ));
                }
                Err(error) => (print_warn)(retry_attempt, error.to_string()),
            };
        }

        log::warn!(
            "GetBalance for {} reached maximum retries - reconnecting",
            self.exchange_account_id
        );

        // TODO: uncomment it after implementation reconnect function
        // await Reconnect();
        return None;
    }

    fn handle_liquidation_price(
        &self,
        currency_pair: CurrencyPair,
        liquidation_price: Price,
        entry_price: Price,
        side: OrderSide,
    ) {
        if !self.symbols.contains_key(&currency_pair) {
            log::warn!(
                "Unknown currency pair {} in handle_liquidation_price for {}",
                currency_pair,
                self.exchange_account_id
            );
            return;
        }

        let event = LiquidationPriceEvent::new(
            time_manager::now(),
            self.exchange_account_id,
            currency_pair,
            liquidation_price,
            entry_price,
            side,
        );

        self.events_channel
            .send_expected(ExchangeEvent::LiquidationPrice(event));

        // TODO: fix it when DataRecorder will be implemented
        // if (exchange.IsRecordingMarketData)
        // {
        //     DataRecorder.Save(liquidationPrice);
        // }
    }

    #[cfg(debug_assertions)]
    pub fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        self.exchange_client
            .get_specific_currency_pair(currency_pair)
    }
}
