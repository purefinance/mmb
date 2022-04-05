use std::sync::{Arc, Weak};

use anyhow::{bail, Context, Result};
use dashmap::DashMap;
use futures::FutureExt;
use itertools::Itertools;
use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::send_expected::SendExpectedByRef;
use mmb_utils::{nothing_to_do, DateTime};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use tokio::sync::{broadcast, oneshot};

use super::commission::Commission;
use super::polling_timeout_manager::PollingTimeoutManager;
use super::symbol::Symbol;
use crate::connectivity::connectivity_manager::GetWSParamsCallback;
use crate::exchanges::common::{ActivePosition, ClosedPosition, MarketId, SpecificCurrencyPair};
use crate::exchanges::events::{
    BalanceUpdateEvent, ExchangeBalance, ExchangeBalancesAndPositions, ExchangeEvent,
    LiquidationPriceEvent, Trade,
};
use crate::exchanges::general::features::{BalancePositionOption, ExchangeFeatures};
use crate::exchanges::general::order::cancel::CancelOrderResult;
use crate::exchanges::general::order::create::CreateOrderResult;
use crate::exchanges::general::request_type::RequestType;
use crate::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use crate::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::misc::derivative_position::DerivativePosition;
use crate::misc::time::time_manager;
use crate::orders::buffered_fills::buffered_canceled_orders_manager::BufferedCanceledOrdersManager;
use crate::orders::buffered_fills::buffered_fills_manager::BufferedFillsManager;
use crate::orders::event::OrderEventType;
use crate::orders::order::OrderSide;
use crate::orders::pool::OrdersPool;
use crate::orders::{order::ExchangeOrderId, pool::OrderRef};
use crate::{
    connectivity::connectivity_manager::WebSocketRole,
    exchanges::common::ExchangeAccountId,
    exchanges::{
        common::{CurrencyPair, ExchangeError},
        traits::ExchangeClient,
    },
    lifecycle::app_lifetime_manager::AppLifetimeManager,
};

use crate::balance_manager::balance_manager::BalanceManager;
use crate::{
    connectivity::{
        connectivity_manager::ConnectivityManager, websocket_connection::WebSocketParams,
    },
    orders::order::ClientOrderId,
};
use crate::{
    exchanges::common::{Amount, CurrencyCode, Price},
    orders::event::OrderEvent,
};
use std::fmt::Debug;
use std::time::Duration;
use tokio::time::sleep;

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

pub struct PriceLevel {
    pub price: Price,
    pub amount: Amount,
}

pub struct OrderBookTop {
    pub ask: Option<PriceLevel>,
    pub bid: Option<PriceLevel>,
}

pub struct Exchange {
    pub exchange_account_id: ExchangeAccountId,
    pub symbols: DashMap<CurrencyPair, Arc<Symbol>>,
    /// Actualised orders data for active order and some late cached orders
    pub orders: Arc<OrdersPool>,
    pub currencies: Mutex<Vec<CurrencyCode>>,
    pub leverage_by_currency_pair: DashMap<CurrencyPair, Decimal>,
    pub order_book_top: DashMap<CurrencyPair, OrderBookTop>,
    pub(super) exchange_client: Box<dyn ExchangeClient>,
    pub(super) features: ExchangeFeatures,
    pub(super) events_channel: broadcast::Sender<ExchangeEvent>,
    pub(super) lifetime_manager: Arc<AppLifetimeManager>,
    pub(super) commission: Commission,
    pub(super) wait_cancel_order: DashMap<ClientOrderId, broadcast::Sender<()>>,
    pub(super) wait_finish_order: DashMap<ClientOrderId, broadcast::Sender<OrderRef>>,
    pub(super) polling_trades_counts: DashMap<ExchangeAccountId, u32>,
    pub(super) polling_timeout_manager: PollingTimeoutManager,
    pub(super) orders_finish_events: DashMap<ClientOrderId, oneshot::Sender<()>>,
    pub(super) orders_created_events: DashMap<ClientOrderId, oneshot::Sender<()>>,
    pub(super) last_trades_update_time: DashMap<MarketId, DateTime>,
    pub(super) last_trades: DashMap<MarketId, Trade>,
    pub(super) timeout_manager: Arc<TimeoutManager>,
    pub(crate) balance_manager: Mutex<Option<Weak<Mutex<BalanceManager>>>>,
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
        lifetime_manager: Arc<AppLifetimeManager>,
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
            lifetime_manager,
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
            buffered_fills_manager: Default::default(),
            buffered_canceled_orders_manager: Default::default(),
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
                None => log::info!("Unable to upgrade weak reference to Exchange instance"),
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_order_cancelled_callback(Box::new(
            move |client_order_id, exchange_order_id, source_type| match exchange_weak.upgrade() {
                Some(exchange) => {
                    exchange.raise_order_cancelled(client_order_id, exchange_order_id, source_type);
                }
                None => log::info!("Unable to upgrade weak reference to Exchange instance"),
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client
            .set_handle_order_filled_callback(Box::new(move |event_data| {
                match exchange_weak.upgrade() {
                    Some(exchange) => exchange.handle_order_filled(event_data),
                    None => log::info!("Unable to upgrade weak reference to Exchange instance"),
                }
            }));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client.set_handle_trade_callback(Box::new(
            move |currency_pair, trade_id, price, quantity, order_side, transaction_time| {
                match exchange_weak.upgrade() {
                    Some(exchange) => {
                        exchange.handle_trade(
                            currency_pair,
                            trade_id,
                            price,
                            quantity,
                            order_side,
                            transaction_time,
                        );
                    }
                    None => log::info!("Unable to upgrade weak reference to Exchange instance"),
                }
            },
        ));

        let exchange_weak = Arc::downgrade(&self);
        self.exchange_client
            .set_send_websocket_message_callback(Box::new(move |role, message| {
                exchange_weak
                    .upgrade()
                    .expect("Unable to upgrade reference to Exchange")
                    .send_websocket_message(role, message)
                    .boxed()
            }));
    }

    fn on_websocket_message(&self, msg: &str) {
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
            .lifetime_manager
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

    async fn send_websocket_message(self: Arc<Self>, role: WebSocketRole, message: String) {
        self.connectivity_manager.send(role, &message).await
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

    pub async fn disconnect(self: Arc<Self>) {
        self.connectivity_manager.clone().disconnect().await
    }

    async fn try_connect(self: Arc<Self>) {
        // TODO IsWebSocketConnecting()
        log::info!("Websocket: Connecting on {}", "test_exchange_id");

        // TODO if UsingWebsocket
        let is_main_websocket_enabled = self
            .exchange_client
            .is_websocket_enabled(WebSocketRole::Main);
        let is_secondary_websocket_enabled = self
            .exchange_client
            .is_websocket_enabled(WebSocketRole::Secondary);
        if !is_main_websocket_enabled && !is_secondary_websocket_enabled {
            return;
        }

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

    pub async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()> {
        self.exchange_client
            .cancel_all_orders(currency_pair)
            .await?;

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
        price: Option<Decimal>,
        cancellation_token: CancellationToken,
    ) -> Option<ClosedPosition> {
        log::info!("Closing position {}", position.id);

        for retry_attempt in 1..=5 {
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

            match self.exchange_client.close_position(position, price).await {
                Ok(closed_position) => {
                    log::info!("Closed position {}", position.id);
                    return Some(closed_position);
                }
                Err(error) => {
                    print_warn(
                        retry_attempt,
                        "close_position",
                        &self.exchange_account_id,
                        error,
                    );
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        log::warn!(
            "Close position with id {} for {} reached maximum retries - reconnecting",
            position.id,
            self.exchange_account_id
        );

        None
    }

    pub async fn get_active_positions(
        &self,
        cancellation_token: CancellationToken,
    ) -> Vec<ActivePosition> {
        for retry_attempt in 1..=5 {
            self.timeout_manager
                .reserve_when_available(
                    self.exchange_account_id,
                    RequestType::GetActivePositions,
                    None,
                    cancellation_token.clone(),
                )
                .expect("Failed to reserve timeout_manager for get_active_positions")
                .await;

            match self.get_active_positions_by_features().await {
                Ok(positions) => return positions,
                Err(error) => {
                    print_warn(
                        retry_attempt,
                        "get_active_positions",
                        &self.exchange_account_id,
                        error,
                    );
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        log::warn!(
            "Get active positions with for {} reached maximum retries - reconnecting",
            self.exchange_account_id
        );

        Vec::new()
    }

    async fn get_active_positions_by_features(&self) -> Result<Vec<ActivePosition>> {
        match self.features.balance_position_option {
            BalancePositionOption::IndividualRequests => {
                self.exchange_client.get_active_positions().await
            }
            BalancePositionOption::SingleRequest => {
                let result = self.exchange_client.get_balance(true).await?;
                Ok(result
                    .positions
                    .context("Positions is none.")?
                    .into_iter()
                    .map(ActivePosition::new)
                    .collect_vec())
            }
            BalancePositionOption::NonDerivative => {
                // TODO Should be implemented manually closing positions for non-derivative exchanges
                Ok(Vec::new())
            }
        }
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
            BalancePositionOption::NonDerivative => {
                return self.exchange_client.get_balance(false).await
            }
            BalancePositionOption::SingleRequest => self.exchange_client.get_balance(true).await?,
            BalancePositionOption::IndividualRequests => {
                let balances_result = self.exchange_client.get_balance(false).await?;

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

                let position_result = self.exchange_client.get_active_positions().await?;

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
                        print_warn(
                            retry_attempt,
                            "get_balance",
                            &self.exchange_account_id,
                            "balances is empty",
                        );
                        continue;
                    }

                    return Some(self.handle_balances_and_positions(
                        self.remove_unknown_currency_pairs(positions, balances),
                    ));
                }
                Err(error) => print_warn(
                    retry_attempt,
                    "get_balance",
                    &self.exchange_account_id,
                    error,
                ),
            };
        }

        log::warn!(
            "GetBalance for {} reached maximum retries - reconnecting",
            self.exchange_account_id
        );

        // TODO: uncomment it after implementation reconnect function
        // await Reconnect();
        None
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
}

/// Helper method only for tests
pub fn get_specific_currency_pair_for_tests(
    exchange: &Exchange,
    currency_pair: CurrencyPair,
) -> SpecificCurrencyPair {
    exchange
        .exchange_client
        .get_specific_currency_pair(currency_pair)
}

fn print_warn(
    retry_attempt: i32,
    fn_name: &str,
    exchange_account_id: &ExchangeAccountId,
    error: impl Debug,
) {
    log::warn!(
        "Failed to {} for {} on retry {}: {:?}",
        fn_name,
        exchange_account_id,
        retry_attempt,
        error
    );
}
