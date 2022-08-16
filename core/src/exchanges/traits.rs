use super::{
    common::CurrencyCode,
    common::{
        ActivePosition, CurrencyPair, ExchangeAccountId, ExchangeError, ExchangeId,
        SpecificCurrencyPair,
    },
    common::{Amount, ClosedPosition, CurrencyId, Price},
    events::{ExchangeBalancesAndPositions, TradeId},
    general::handlers::handle_order_filled::FillEvent,
    general::symbol::BeforeAfter,
    general::{order::get_order_trades::OrderTrade, symbol::Symbol},
    timeouts::requests_timeout_manager_factory::RequestTimeoutArguments,
};
use crate::exchanges::events::ExchangeEvent;
use crate::exchanges::general::exchange::{Exchange, RequestResult};
use crate::exchanges::general::features::ExchangeFeatures;
use crate::exchanges::general::order::cancel::CancelOrderResult;
use crate::exchanges::general::order::create::CreateOrderResult;
use crate::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::lifecycle::app_lifetime_manager::AppLifetimeManager;
use crate::orders::fill::EventSourceType;
use crate::orders::order::{
    ClientOrderId, ExchangeOrderId, OrderCancelling, OrderInfo, OrderInfoExtensionData,
};
use crate::orders::pool::OrdersPool;
use crate::settings::ExchangeSettings;
use crate::{connectivity::WebSocketRole, orders::order::OrderSide};
use crate::{exchanges::general::exchange::BoxExchangeClient, orders::pool::OrderRef};
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use mmb_utils::DateTime;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::broadcast;
use url::Url;

// Implementation of rest API client
#[async_trait]
pub trait ExchangeClient: Support {
    async fn create_order(&self, order: &OrderRef) -> CreateOrderResult;

    async fn cancel_order(&self, order: OrderCancelling) -> CancelOrderResult;

    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) -> Result<()>;

    async fn get_open_orders(&self) -> Result<Vec<OrderInfo>>;

    async fn get_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>>;

    async fn get_order_info(&self, order: &OrderRef) -> Result<OrderInfo, ExchangeError>;

    async fn close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<ClosedPosition>;

    async fn get_active_positions(&self) -> Result<Vec<ActivePosition>>;

    async fn get_balance(&self, is_spot: bool) -> Result<ExchangeBalancesAndPositions>;

    async fn get_my_trades(
        &self,
        symbol: &Symbol,
        last_date_time: Option<DateTime>,
    ) -> RequestResult<Vec<OrderTrade>>;

    async fn build_all_symbols(&self) -> Result<Vec<Arc<Symbol>>>;
}

pub type OrderCreatedCb =
    Box<dyn Fn(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>;

pub type OrderCancelledCb =
    Box<dyn Fn(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>;

pub type HandleTradeCb =
    Box<dyn Fn(CurrencyPair, TradeId, Price, Amount, OrderSide, DateTime) + Send + Sync>;

pub type HandleOrderFilledCb = Box<dyn Fn(FillEvent) + Send + Sync>;

pub type SendWebsocketMessageCb = Box<dyn Fn(WebSocketRole, String) -> Result<()> + Send + Sync>;

#[async_trait]
pub trait Support: Send + Sync {
    /// Needed to call the `downcast_ref` method
    fn as_any(&self) -> &(dyn Any + Send + Sync + 'static);

    async fn initialized(&self, _exchange: Arc<Exchange>) {}

    fn on_websocket_message(&self, msg: &str) -> Result<()>;
    fn on_connecting(&self) -> Result<()>;
    fn on_disconnected(&self) -> Result<()>;
    fn set_send_websocket_message_callback(&self, callback: SendWebsocketMessageCb);

    fn set_order_created_callback(&mut self, callback: OrderCreatedCb);

    fn set_order_cancelled_callback(&mut self, callback: OrderCancelledCb);

    fn set_handle_order_filled_callback(&mut self, callback: HandleOrderFilledCb);

    fn set_handle_trade_callback(&mut self, callback: HandleTradeCb);

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>);

    fn is_websocket_enabled(&self, role: WebSocketRole) -> bool;

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Url>;

    fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair;

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode>;

    fn should_log_message(&self, message: &str) -> bool;

    fn log_unknown_message(&self, exchange_account_id: ExchangeAccountId, message: &str) {
        log::info!("Unknown message for {exchange_account_id}: {message}");
    }

    fn get_balance_reservation_currency_code(
        &self,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> CurrencyCode {
        symbol.get_trade_code(side, BeforeAfter::Before)
    }

    fn get_settings(&self) -> &ExchangeSettings;

    fn get_initial_extension_data(&self) -> Option<Box<dyn OrderInfoExtensionData>> {
        None
    }
}

pub struct ExchangeClientBuilderResult {
    pub client: BoxExchangeClient,
    pub features: ExchangeFeatures,
}

pub trait ExchangeClientBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
        timeout_manager: Arc<TimeoutManager>,
        orders: Arc<OrdersPool>,
    ) -> ExchangeClientBuilderResult;

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments;

    fn get_exchange_id(&self) -> ExchangeId;
}
