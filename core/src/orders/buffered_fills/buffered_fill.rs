use mmb_utils::DateTime;
use rust_decimal::Decimal;

use crate::exchanges::general::handlers::handle_order_filled::FillAmount;
use crate::{
    exchanges::{
        common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId, Price},
        events::TradeId,
        general::handlers::handle_order_filled::FillEvent,
    },
    orders::{
        fill::{EventSourceType, OrderFillType},
        order::{ClientOrderId, ExchangeOrderId, OrderRole, OrderSide},
    },
};

#[derive(Clone, Debug)]
pub struct BufferedFill {
    pub exchange_account_id: ExchangeAccountId,
    pub trade_id: TradeId,
    pub exchange_order_id: ExchangeOrderId,
    pub fill_price: Price,
    pub fill_amount: Amount,
    pub is_diff: bool,
    pub total_filled_amount: Option<Amount>,
    pub order_role: Option<OrderRole>,
    pub commission_currency_code: CurrencyCode,
    pub commission_rate: Option<Decimal>,
    pub commission_amount: Option<Amount>,
    pub side: Option<OrderSide>,
    pub fill_type: OrderFillType,
    pub trade_currency_pair: CurrencyPair,
    pub fill_date: Option<DateTime>,
    pub event_source_type: EventSourceType,
}

impl BufferedFill {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        trade_id: TradeId,
        exchange_order_id: ExchangeOrderId,
        fill_price: Price,
        fill_amount: Amount,
        is_diff: bool,
        total_filled_amount: Option<Amount>,
        order_role: Option<OrderRole>,
        commission_currency_code: CurrencyCode,
        commission_rate: Option<Decimal>,
        commission_amount: Option<Amount>,
        side: Option<OrderSide>,
        fill_type: OrderFillType,
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
            order_role,
            commission_currency_code,
            commission_rate,
            commission_amount,
            side,
            fill_type,
            trade_currency_pair,
            fill_date,
            event_source_type,
        }
    }

    pub fn to_fill_event_data(
        &self,
        order_amount: Option<Decimal>,
        client_order_id: ClientOrderId,
    ) -> FillEvent {
        let fill_amount = if self.is_diff {
            FillAmount::Incremental {
                fill_amount: self.fill_amount,
                total_filled_amount: self.total_filled_amount,
            }
        } else {
            FillAmount::Total {
                total_filled_amount: self.fill_amount,
            }
        };

        FillEvent {
            source_type: self.event_source_type,
            trade_id: Some(self.trade_id.clone()),
            client_order_id: Some(client_order_id),
            exchange_order_id: self.exchange_order_id.clone(),
            fill_price: self.fill_price,
            fill_amount,
            order_role: self.order_role,
            commission_currency_code: Some(self.commission_currency_code),
            commission_rate: self.commission_rate,
            commission_amount: self.commission_amount,
            fill_type: self.fill_type,
            trade_currency_pair: Some(self.trade_currency_pair),
            order_side: self.side,
            order_amount,
            fill_date: self.fill_date,
        }
    }
}
