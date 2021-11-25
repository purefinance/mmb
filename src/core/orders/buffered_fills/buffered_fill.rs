use rust_decimal::Decimal;

use crate::core::{
    exchanges::{
        common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price},
        events::TradeId,
    },
    orders::{
        fill::{EventSourceType, OrderFillType},
        order::{ExchangeOrderId, OrderSide},
    },
    DateTime,
};

#[derive(Clone)]
pub struct BufferedFill {
    pub exchange_account_id: ExchangeAccountId,
    pub trade_id: TradeId,
    pub exchange_order_id: ExchangeOrderId,
    pub fill_price: Price,
    pub fill_amount: Amount,
    pub is_diff: bool,
    pub total_filled_amount: Option<Amount>,
    pub is_maker: Option<bool>,
    pub commission_currency_code: CurrencyCode,
    pub commission_rate: Option<Decimal>,
    pub commission_amount: Option<Amount>,
    pub side: Option<OrderSide>,
    pub order_fill_type: OrderFillType,
    pub trade_currency_pair: CurrencyPair,
    pub fill_date: Option<DateTime>,
    pub event_source_type: EventSourceType,
}

impl BufferedFill {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        trade_id: TradeId,
        exchange_order_id: ExchangeOrderId,
        fill_price: Price,
        fill_amount: Amount,
        is_diff: bool,
        total_filled_amount: Option<Amount>,
        is_maker: Option<bool>,
        commission_currency_code: CurrencyCode,
        commission_rate: Option<Decimal>,
        commission_amount: Option<Amount>,
        side: Option<OrderSide>,
        order_fill_type: OrderFillType,
        trade_currency_pair: CurrencyPair,
        fill_date: Option<DateTime>,
        event_source_type: EventSourceType,
    ) -> Self {
        Self {
            exchange_account_id,
            trade_id,
            exchange_order_id,
            fill_price,
            fill_amount,
            is_diff,
            total_filled_amount,
            is_maker,
            commission_currency_code,
            commission_rate,
            commission_amount,
            side,
            order_fill_type,
            trade_currency_pair,
            fill_date,
            event_source_type,
        }
    }
}
