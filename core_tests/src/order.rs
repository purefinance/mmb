use mmb_core::exchanges::common::{Amount, Price};
use mmb_core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::general::exchange::RequestResult;
use mmb_core::orders::order::*;
use mmb_core::orders::pool::OrderRef;

use mmb_utils::cancellation_token::CancellationToken;
use mmb_utils::DateTime;

use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::Duration;

use mmb_utils::infrastructure::with_timeout;
use std::sync::Arc;

/// This struct needed for creating an orders in tests.
/// You can create an object and change some fields if it's necessary
/// and than use create_order function for making order in selected Exchange.
///
/// ```no_run
/// use core_tests::order::OrderProxy;
/// use mmb_core::exchanges::common::ExchangeAccountId;
/// use mmb_core::exchanges::common::Price;
/// use mmb_core::exchanges::general::exchange::Exchange;
/// use mmb_utils::cancellation_token::CancellationToken;
/// use rust_decimal_macros::dec;
/// use std::sync::Arc;
///
/// async fn example(exchange_account_id: ExchangeAccountId, exchange: Arc<Exchange>, price: Price) {
///     let mut order_proxy = OrderProxy::new(
///         exchange_account_id,
///         Some("FromExample".to_owned()),
///         CancellationToken::default(),
///         price,
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
    timeout: Duration,
}

impl OrderProxy {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        strategy_name: Option<String>,
        cancellation_token: CancellationToken,
        price: Price,
    ) -> Self {
        Self {
            client_order_id: ClientOrderId::unique_id(),
            init_time: Utc::now(),
            exchange_account_id,
            currency_pair: OrderProxy::default_currency_pair(),
            order_type: OrderType::Limit,
            side: OrderSide::Buy,
            amount: OrderProxy::default_amount(),
            execution_type: OrderExecutionType::None,
            reservation_id: None,
            signal_id: None,
            strategy_name: strategy_name.unwrap_or("OrderTest".to_owned()),
            price,
            cancellation_token,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn default_currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes("cnd".into(), "btc".into())
    }

    pub fn default_amount() -> Decimal {
        dec!(1000)
    }

    pub fn default_price() -> Decimal {
        dec!(0.0000001)
    }

    pub fn make_header(&self) -> Arc<OrderHeader> {
        OrderHeader::new(
            self.client_order_id.clone(),
            self.init_time,
            self.exchange_account_id,
            self.currency_pair,
            self.order_type,
            self.side,
            self.amount,
            self.execution_type,
            self.reservation_id.clone(),
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
            exchange.create_order(&to_create, None, self.cancellation_token.clone()),
        )
        .await
    }

    pub async fn cancel_order_or_fail(&self, order_ref: &OrderRef, exchange: Arc<Exchange>) {
        let header = self.make_header();
        let exchange_order_id = order_ref.exchange_order_id().expect("in test");
        let order_to_cancel = OrderCancelling {
            header: header.clone(),
            exchange_order_id,
        };

        let cancel_outcome = exchange
            .cancel_order(&order_to_cancel, CancellationToken::default())
            .await
            .expect("in test")
            .expect("in test");

        if let RequestResult::Success(gotten_client_order_id) = cancel_outcome.outcome {
            assert_eq!(gotten_client_order_id, self.client_order_id);
        }
    }
}
