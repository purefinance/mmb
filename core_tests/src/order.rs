use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_domain::market::ExchangeAccountId;
use mmb_domain::order::pool::OrderRef;
use mmb_domain::order::snapshot::Amount;
use mmb_domain::order::snapshot::*;

use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::DateTime;

use anyhow::Result;
use chrono::Utc;
use tokio::time::Duration;

use mmb_domain::market::CurrencyPair;
use mmb_domain::order::snapshot::Price;
use mmb_utils::infrastructure::with_timeout;
use std::sync::Arc;

/// This struct needed for creating an orders in tests.
/// You can create an object and change some fields if it's necessary
/// and than use create_order function for making order in selected Exchange.
///
/// ```no_run
/// use mmb_domain::market::ExchangeAccountId;
/// use mmb_domain::market::{ };
/// use mmb_core::exchanges::general::exchange::Exchange;
/// use mmb_utils::cancellation_token::CancellationToken;
/// use rust_decimal_macros::dec;
/// use std::sync::Arc;
/// use core_tests::order::OrderProxy;
/// use mmb_domain::order::snapshot::{Amount, Price};
///
/// async fn example(exchange_account_id: ExchangeAccountId, exchange: Arc<Exchange>, price: Price, amount: Amount) {
///     let mut order_proxy = OrderProxy::new(
///         exchange_account_id,
///         Some("FromExample".to_owned()),
///         CancellationToken::default(),
///         price,
///         amount,
///     );
///     order_proxy.amount = dec!(5000); // Optional amount changing
///     let created_order = order_proxy.create_order(exchange.clone()).await;
/// }
/// ```
pub struct OrderProxy {
    pub client_order_id: ClientOrderId,
    pub init_time: DateTime,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub amount: Amount,
    pub execution_type: OrderExecutionType,
    pub reservation_id: Option<ReservationId>,
    pub signal_id: Option<String>,
    pub strategy_name: String,

    pub price: Price,
    pub cancellation_token: CancellationToken,
    pub timeout: Duration,
}

impl OrderProxy {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        strategy_name: Option<String>,
        cancellation_token: CancellationToken,
        price: Price,
        amount: Amount,
    ) -> Self {
        Self {
            client_order_id: ClientOrderId::unique_id(),
            init_time: Utc::now(),
            exchange_account_id,
            currency_pair: OrderProxy::default_currency_pair(),
            order_type: OrderType::Limit,
            side: OrderSide::Buy,
            amount,
            execution_type: OrderExecutionType::None,
            reservation_id: None,
            signal_id: None,
            strategy_name: strategy_name.unwrap_or_else(|| "OrderTest".to_owned()),
            price,
            cancellation_token,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn default_currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes("btc".into(), "usdt".into())
    }

    pub fn make_header(&self) -> Arc<OrderHeader> {
        OrderHeader::new(
            self.client_order_id.clone(),
            self.exchange_account_id,
            self.currency_pair,
            self.order_type,
            self.side,
            self.amount,
            self.execution_type,
            self.reservation_id,
            self.signal_id.clone(),
            self.strategy_name.clone(),
        )
    }

    pub async fn create_order(&self, exchange: Arc<Exchange>) -> Result<OrderRef> {
        let header = self.make_header();
        let to_create = OrderCreating {
            price: self.price,
            header: header.clone(),
        };

        with_timeout(
            self.timeout,
            exchange.create_order(to_create, None, self.cancellation_token.clone()),
        )
        .await
    }

    pub async fn cancel_order_or_fail(&self, order_ref: &OrderRef, exchange: Arc<Exchange>) {
        let header = self.make_header();
        let exchange_order_id = order_ref.exchange_order_id().expect("in test");
        let order_to_cancel = OrderCancelling {
            header: header.clone(),
            exchange_order_id,
            extension_data: order_ref.fn_ref(|s| s.extension_data.clone()),
        };

        order_ref.fn_mut(|order| order.set_status(OrderStatus::Canceling, Utc::now()));

        let cancel_outcome = exchange
            .cancel_order(order_to_cancel, CancellationToken::default())
            .await
            .expect("in test");

        if let RequestResult::Success(gotten_client_order_id) = cancel_outcome.outcome {
            assert_eq!(gotten_client_order_id, self.client_order_id);
        }
    }
}

pub struct OrderProxyBuilder {
    exchange_account_id: ExchangeAccountId,
    currency_pair: CurrencyPair,
    order_type: OrderType,
    side: OrderSide,
    amount: Amount,
    strategy_name: String,
    price: Price,
    cancellation_token: CancellationToken,
    timeout: Duration,
}

impl OrderProxyBuilder {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        strategy_name: Option<String>,
        price: Price,
        amount: Amount,
    ) -> OrderProxyBuilder {
        Self {
            exchange_account_id,
            currency_pair: OrderProxy::default_currency_pair(),
            order_type: OrderType::Limit,
            strategy_name: strategy_name.unwrap_or_else(|| "OrderTest".to_owned()),
            cancellation_token: CancellationToken::default(),
            price,
            amount,
            side: OrderSide::Buy,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn currency_pair(mut self, currency_pair: CurrencyPair) -> OrderProxyBuilder {
        self.currency_pair = currency_pair;
        self
    }

    pub fn side(mut self, side: OrderSide) -> OrderProxyBuilder {
        self.side = side;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> OrderProxyBuilder {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> OrderProxy {
        OrderProxy {
            client_order_id: ClientOrderId::unique_id(),
            init_time: Utc::now(),
            exchange_account_id: self.exchange_account_id,
            currency_pair: self.currency_pair,
            order_type: self.order_type,
            side: self.side,
            amount: self.amount,
            execution_type: OrderExecutionType::None,
            reservation_id: None,
            signal_id: None,
            strategy_name: self.strategy_name,
            price: self.price,
            cancellation_token: self.cancellation_token,
            timeout: self.timeout,
        }
    }
}
