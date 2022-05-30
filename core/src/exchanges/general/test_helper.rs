#![cfg(test)]
use std::{collections::HashMap, sync::Arc};

use crate::{
    connectivity::WebSocketRole,
    exchanges::{
        common::{
            ActivePosition, Amount, ClosedPosition, CurrencyCode, CurrencyId, CurrencyPair,
            ExchangeAccountId, ExchangeError, Price, SpecificCurrencyPair,
        },
        events::{AllowedEventSourceType, ExchangeBalancesAndPositions, ExchangeEvent},
        general::{
            commission::{Commission, CommissionForType},
            exchange::Exchange,
            features::{
                ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption,
                RestFillsFeatures, WebSocketOptions,
            },
            symbol::{Precision, Symbol},
        },
        timeouts::{
            requests_timeout_manager_factory::RequestTimeoutArguments,
            timeout_manager::TimeoutManager,
        },
        traits::{ExchangeClient, HandleTradeCb, OrderCancelledCb, OrderCreatedCb, Support},
    },
    lifecycle::app_lifetime_manager::AppLifetimeManager,
    orders::{
        order::{
            ClientOrderId, OrderCancelling, OrderInfo, OrderRole, OrderSide, OrderSnapshot,
            OrderType,
        },
        pool::{OrderRef, OrdersPool},
    },
    settings::ExchangeSettings,
};
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal_macros::dec;
use tokio::sync::broadcast;
use url::Url;

use crate::exchanges::exchange_blocker::ExchangeBlocker;
use crate::exchanges::general::exchange::RequestResult;
use crate::exchanges::general::order::cancel::CancelOrderResult;
use crate::exchanges::general::order::create::CreateOrderResult;
use crate::exchanges::traits::{HandleOrderFilledCb, SendWebsocketMessageCb};
use mmb_utils::{cancellation_token::CancellationToken, DateTime};

use super::{order::get_order_trades::OrderTrade, symbol::BeforeAfter};

pub struct TestClient;

#[async_trait]
impl ExchangeClient for TestClient {
    async fn create_order(&self, _order: &OrderRef) -> CreateOrderResult {
        unimplemented!("doesn't need in UT")
    }

    async fn cancel_order(&self, _order: OrderCancelling) -> CancelOrderResult {
        unimplemented!("doesn't need in UT")
    }

    async fn cancel_all_orders(&self, _currency_pair: CurrencyPair) -> Result<()> {
        unimplemented!("doesn't need in UT")
    }

    async fn get_open_orders(&self) -> Result<Vec<OrderInfo>> {
        unimplemented!("doesn't need in UT")
    }

    async fn get_open_orders_by_currency_pair(
        &self,
        _currency_pair: CurrencyPair,
    ) -> Result<Vec<OrderInfo>> {
        unimplemented!("doesn't need in UT")
    }

    async fn get_order_info(&self, _order: &OrderRef) -> Result<OrderInfo, ExchangeError> {
        unimplemented!("doesn't need in UT")
    }

    async fn close_position(
        &self,
        _position: &ActivePosition,
        _price: Option<Price>,
    ) -> Result<ClosedPosition> {
        unimplemented!("doesn't need in UT")
    }

    async fn get_active_positions(&self) -> Result<Vec<ActivePosition>> {
        unimplemented!("doesn't need in UT")
    }

    async fn get_balance(&self, _is_spot: bool) -> Result<ExchangeBalancesAndPositions> {
        unimplemented!("doesn't need in UT")
    }

    async fn get_my_trades(
        &self,
        _symbol: &Symbol,
        _last_date_time: Option<DateTime>,
    ) -> Result<RequestResult<Vec<OrderTrade>>> {
        unimplemented!("doesn't need in UT")
    }

    async fn build_all_symbols(&self) -> Result<Vec<Arc<Symbol>>> {
        unimplemented!("doesn't need in UT")
    }
}

#[async_trait]
impl Support for TestClient {
    fn on_websocket_message(&self, _msg: &str) -> Result<()> {
        unimplemented!("doesn't need in UT")
    }
    fn on_connecting(&self) -> Result<()> {
        unimplemented!("doesn't need in UT")
    }

    fn set_send_websocket_message_callback(&self, _callback: SendWebsocketMessageCb) {}

    fn set_order_created_callback(&mut self, _callback: OrderCreatedCb) {}

    fn set_order_cancelled_callback(&mut self, _callback: OrderCancelledCb) {}

    fn set_handle_order_filled_callback(&mut self, _callback: HandleOrderFilledCb) {}

    fn set_handle_trade_callback(&mut self, _callback: HandleTradeCb) {}

    fn set_traded_specific_currencies(&self, _currencies: Vec<SpecificCurrencyPair>) {}

    fn is_websocket_enabled(&self, _role: WebSocketRole) -> bool {
        unimplemented!("doesn't need in UT")
    }

    async fn create_ws_url(&self, _role: WebSocketRole) -> Result<Url> {
        unimplemented!("doesn't need in UT")
    }

    fn get_specific_currency_pair(&self, _currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        unimplemented!("doesn't need in UT")
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        unimplemented!("doesn't need in UT")
    }

    fn should_log_message(&self, _message: &str) -> bool {
        unimplemented!("doesn't need in UT")
    }

    fn log_unknown_message(&self, exchange_account_id: ExchangeAccountId, message: &str) {
        log::info!("Unknown message for {}: {}", exchange_account_id, message);
    }

    fn get_balance_reservation_currency_code(
        &self,
        symbol: Arc<Symbol>,
        side: OrderSide,
    ) -> CurrencyCode {
        symbol.get_trade_code(side, BeforeAfter::Before)
    }

    fn get_settings(&self) -> &ExchangeSettings {
        unimplemented!("doesn't need in UT")
    }
}

pub(crate) fn get_test_exchange(
    is_derivative: bool,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let base_currency_code = "PHB";
    let quote_currency_code = "BTC";
    get_test_exchange_by_currency_codes(is_derivative, base_currency_code, quote_currency_code)
}

pub(crate) fn get_test_exchange_by_currency_codes_and_amount_code(
    is_derivative: bool,
    base_currency_code: &str,
    quote_currency_code: &str,
    amount_currency_code: &str,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let price_tick = dec!(0.1);
    let symbol = Arc::new(Symbol::new(
        false,
        is_derivative,
        base_currency_code.into(),
        base_currency_code.into(),
        quote_currency_code.into(),
        quote_currency_code.into(),
        None,
        None,
        None,
        None,
        None,
        amount_currency_code.into(),
        None,
        Precision::ByTick { tick: price_tick },
        Precision::ByTick { tick: dec!(0) },
    ));
    get_test_exchange_with_symbol(symbol)
}

pub(crate) fn get_test_exchange_by_currency_codes(
    is_derivative: bool,
    base_currency_code: &str,
    quote_currency_code: &str,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let amount_currency_code = if is_derivative {
        quote_currency_code
    } else {
        base_currency_code
    };
    get_test_exchange_by_currency_codes_and_amount_code(
        is_derivative,
        base_currency_code,
        quote_currency_code,
        amount_currency_code,
    )
}

pub(crate) fn get_test_exchange_with_symbol(
    symbol: Arc<Symbol>,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let exchange_account_id = ExchangeAccountId::new("local_exchange_account_id", 0);
    get_test_exchange_with_symbol_and_id(symbol, exchange_account_id)
}
pub(crate) fn get_test_exchange_with_symbol_and_id(
    symbol: Arc<Symbol>,
    exchange_account_id: ExchangeAccountId,
) -> (Arc<Exchange>, broadcast::Receiver<ExchangeEvent>) {
    let lifetime_manager = AppLifetimeManager::new(CancellationToken::new());
    let (tx, rx) = broadcast::channel(10);

    let exchange_client = Box::new(TestClient);
    let referral_reward = dec!(40);
    let commission = Commission::new(
        CommissionForType::new(dec!(0.1), referral_reward),
        CommissionForType::new(dec!(0.2), referral_reward),
    );

    let exchange_blocker = ExchangeBlocker::new(vec![exchange_account_id]);

    let exchange = Exchange::new(
        exchange_account_id,
        exchange_client,
        OrdersPool::new(),
        ExchangeFeatures::new(
            OpenOrdersType::AllCurrencyPair,
            RestFillsFeatures::default(),
            OrderFeatures::default(),
            OrderTradeOption::default(),
            WebSocketOptions::default(),
            false,
            true,
            AllowedEventSourceType::default(),
            AllowedEventSourceType::default(),
        ),
        RequestTimeoutArguments::from_requests_per_minute(1200),
        tx,
        lifetime_manager,
        TimeoutManager::new(HashMap::new()),
        Arc::downgrade(&exchange_blocker),
        commission,
    );

    exchange
        .leverage_by_currency_pair
        .insert(symbol.currency_pair(), dec!(1));
    exchange.currencies.lock().push(symbol.base_currency_code());
    exchange
        .currencies
        .lock()
        .push(symbol.quote_currency_code());
    exchange.symbols.insert(symbol.currency_pair(), symbol);

    (exchange, rx)
}

pub(crate) fn create_order_ref(
    client_order_id: &ClientOrderId,
    role: Option<OrderRole>,
    exchange_account_id: ExchangeAccountId,
    currency_pair: CurrencyPair,
    price: Price,
    amount: Amount,
    side: OrderSide,
) -> OrderRef {
    let order = OrderSnapshot::with_params(
        client_order_id.clone(),
        OrderType::Liquidation,
        role,
        exchange_account_id,
        currency_pair,
        price,
        amount,
        side,
        None,
        "StrategyInUnitTests",
    );

    let order_pool = OrdersPool::new();
    order_pool.add_snapshot_initial(Arc::new(RwLock::new(order)));
    let order_ref = order_pool
        .cache_by_client_id
        .get(&client_order_id)
        .expect("in test");

    order_ref.clone()
}

pub(crate) fn try_add_snapshot_by_exchange_id(exchange: &Exchange, order_ref: &OrderRef) {
    if let Some(exchange_order_id) = order_ref.exchange_order_id() {
        let _ = exchange
            .orders
            .cache_by_exchange_id
            .insert(exchange_order_id.clone(), order_ref.clone());
    }
}
