use mmb_lib::core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_lib::core::exchanges::general::exchange::Exchange;
use mmb_lib::core::lifecycle::cancellation_token::CancellationToken;
use mmb_lib::core::orders::order::*;
use mmb_lib::core::orders::pool::OrderRef;

use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::time::Duration;

use std::sync::Arc;

pub struct Order {
    pub header: Arc<OrderHeader>,
    pub to_create: OrderCreating,
    pub cancellation_token: CancellationToken,

    timeout: Duration,
}

impl Order {
    pub fn new(
        test_order_client_id: Option<ClientOrderId>,
        exchange_account_id: ExchangeAccountId,
        strategy_name: Option<String>,
        cancellation_token: CancellationToken,
    ) -> Order {
        let header = OrderHeader::new(
            test_order_client_id.unwrap_or(ClientOrderId::unique_id()),
            Utc::now(),
            exchange_account_id,
            Order::default_currency_pair(),
            OrderType::Limit,
            OrderSide::Buy,
            Order::default_amount(),
            OrderExecutionType::None,
            None,
            None,
            strategy_name.unwrap_or("OrderTest".to_owned()),
        );
        Order {
            header: header.clone(),
            to_create: OrderCreating {
                header: header.clone(),
                price: Order::default_price(),
            },
            cancellation_token: cancellation_token,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn default_currency_pair() -> CurrencyPair {
        CurrencyPair::from_codes("phb".into(), "btc".into())
    }

    pub fn default_amount() -> Decimal {
        dec!(2000)
    }

    pub fn default_price() -> Decimal {
        dec!(0.0000001)
    }

    pub async fn create(&self, exchange: &Exchange) -> anyhow::Result<OrderRef> {
        let _ = exchange
            .cancel_all_orders(self.header.currency_pair.clone())
            .await
            .expect("in test");
        let created_order_fut =
            exchange.create_order(&self.to_create, self.cancellation_token.clone());

        let created_order = tokio::select! {
            created_order = created_order_fut => created_order,
            _ = tokio::time::sleep(self.timeout) => panic!("Timeout {} secs is exceeded", self.timeout.as_secs())
        };
        created_order
    }
}
