use crate::events::TradeId;
use crate::market::CurrencyCode;
use crate::order::snapshot::{OrderFillRole, OrderSide};
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::order::snapshot::ClientOrderFillId;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum OrderFillType {
    UserTrade = 1,
    Liquidation = 2,
    Funding = 3,
    ClosePosition = 4,
}

impl OrderFillType {
    pub fn is_special(&self) -> bool {
        use OrderFillType::*;
        matches!(self, Liquidation | ClosePosition)
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum EventSourceType {
    RestFallback = 1,
    Rest = 2,
    WebSocket = 3,
    Rpc = 4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFill {
    id: Uuid,
    client_order_fill_id: Option<ClientOrderFillId>,
    receive_time: DateTime,
    fill_type: OrderFillType,

    trade_id: Option<TradeId>,
    price: Decimal,
    amount: Decimal,
    cost: Decimal,
    role: OrderFillRole,
    commission_currency_code: CurrencyCode,
    commission_amount: Decimal,
    referral_reward_amount: Decimal,

    /// ConvertedCommissionCurrencyCode is CommissionCurrencyCode if  CommissionCurrencyCode is equal to base or quote
    /// Otherwise it is equal to quote currency code (for example, in the case of BNB fee discount) after conversion
    /// by HandleOrderFilled
    converted_commission_currency_code: CurrencyCode,
    converted_commission_amount: Decimal,
    expected_converted_commission_amount: Decimal,

    is_incremental_fill: bool,
    event_source_type: Option<EventSourceType>,
    side: Option<OrderSide>,
}

impl OrderFill {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        client_order_fill_id: Option<ClientOrderFillId>,
        receive_time: DateTime,
        fill_type: OrderFillType,
        trade_id: Option<TradeId>,
        price: Decimal,
        amount: Decimal,
        cost: Decimal,
        role: OrderFillRole,
        commission_currency_code: CurrencyCode,
        commission_amount: Decimal,
        referral_reward_amount: Decimal,
        converted_commission_currency_code: CurrencyCode,
        converted_commission_amount: Decimal,
        expected_converted_commission_amount: Decimal,
        is_incremental_fill: bool,
        event_source_type: Option<EventSourceType>,
        side: Option<OrderSide>,
    ) -> Self {
        OrderFill {
            id,
            client_order_fill_id,
            receive_time,
            fill_type,
            trade_id,
            price,
            amount,
            cost,
            role,
            commission_currency_code,
            commission_amount,
            referral_reward_amount,
            converted_commission_currency_code,
            converted_commission_amount,
            expected_converted_commission_amount,
            is_incremental_fill,
            event_source_type,
            side,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn receive_time(&self) -> DateTime {
        self.receive_time
    }
    pub fn fill_type(&self) -> OrderFillType {
        self.fill_type
    }
    pub fn trade_id(&self) -> Option<&TradeId> {
        self.trade_id.as_ref()
    }
    pub fn price(&self) -> Decimal {
        self.price
    }
    pub fn amount(&self) -> Decimal {
        self.amount
    }
    pub fn cost(&self) -> Decimal {
        self.cost
    }
    pub fn role(&self) -> OrderFillRole {
        self.role
    }
    pub fn commission_currency_code(&self) -> CurrencyCode {
        self.commission_currency_code
    }
    pub fn commission_amount(&self) -> Decimal {
        self.commission_amount
    }
    pub fn referral_reward_amount(&self) -> Decimal {
        self.referral_reward_amount
    }
    pub fn converted_commission_currency_code(&self) -> CurrencyCode {
        self.converted_commission_currency_code
    }
    pub fn converted_commission_amount(&self) -> Decimal {
        self.converted_commission_amount
    }
    pub fn expected_converted_commission_amount(&self) -> Decimal {
        self.expected_converted_commission_amount
    }
    pub fn is_incremental_fill(&self) -> bool {
        self.is_incremental_fill
    }
    pub fn event_source_type(&self) -> Option<EventSourceType> {
        self.event_source_type
    }
    pub fn side(&self) -> Option<OrderSide> {
        self.side
    }
    pub fn client_order_fill_id(&self) -> &Option<ClientOrderFillId> {
        &self.client_order_fill_id
    }

    pub fn set_client_order_fill_id(&mut self, input: ClientOrderFillId) {
        self.client_order_fill_id = Some(input);
    }
}
