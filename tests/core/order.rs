use mmb_lib::core::exchanges::common::{CurrencyPair, ExchangeAccountId};
use mmb_lib::core::orders::order::*;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use std::sync::Arc;

pub struct Order {
    pub header: Arc<OrderHeader>,
    pub to_create: OrderCreating,
}

impl Order {
    pub fn new(
        test_order_client_id: Option<ClientOrderId>,
        exchange_account_id: ExchangeAccountId,
        strategy_name: Option<String>,
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
}
