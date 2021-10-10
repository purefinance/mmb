pub mod executor;
pub mod trade_limit;
mod trading_context_calculation;

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use derive_getters::Getters;
use enum_map::{enum_map, EnumMap};
use log::{error, info};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::core::exchanges::common::{
    Amount, CurrencyPair, ExchangeAccountId, Price, TradePlace, TradePlaceAccount,
};
use crate::core::exchanges::timeouts::requests_timeout_manager::RequestGroupId;
use crate::core::explanation::{Explanation, WithExplanation};
use crate::core::orders::order::{ClientOrderId, OrderRole, OrderSide};
use crate::core::orders::pool::OrderRef;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SmallOrder {
    pub price: Price,
    pub amount: Amount,
}

impl SmallOrder {
    pub fn new(price: Price, amount: Amount) -> Self {
        SmallOrder { price, amount }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TradeDirection {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
    pub side: OrderSide,
}

impl Display for TradeDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} {}",
            self.exchange_account_id, self.currency_pair, self.side
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TradeDisposition {
    pub direction: TradeDirection,
    pub order: SmallOrder,
}

impl TradeDisposition {
    pub fn new(
        trade_place_account: TradePlaceAccount,
        side: OrderSide,
        price: Price,
        amount: Amount,
    ) -> Self {
        TradeDisposition {
            direction: TradeDirection {
                exchange_account_id: trade_place_account.exchange_account_id,
                currency_pair: trade_place_account.currency_pair,
                side,
            },
            order: SmallOrder::new(price, amount),
        }
    }

    pub fn exchange_account_id(&self) -> ExchangeAccountId {
        self.direction.exchange_account_id
    }

    pub fn currency_pair(&self) -> CurrencyPair {
        self.direction.currency_pair
    }

    pub fn side(&self) -> OrderSide {
        self.direction.side
    }

    pub fn trade_place(&self) -> TradePlace {
        let direction = &self.direction;
        TradePlace::new(
            self.direction.exchange_account_id.exchange_id,
            direction.currency_pair,
        )
    }

    pub fn trade_place_account(&self) -> TradePlaceAccount {
        let direction = &self.direction;
        TradePlaceAccount::new(self.direction.exchange_account_id, direction.currency_pair)
    }

    pub fn price(&self) -> Price {
        self.order.price
    }

    pub fn amount(&self) -> Amount {
        self.order.amount
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TradeCycle {
    pub order_role: OrderRole,
    pub strategy_name: String,
    pub disposition: TradeDisposition,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TradingContextBySide {
    pub max_amount: Amount,
    pub estimating: Vec<WithExplanation<Option<TradeCycle>>>,
}

impl TradingContextBySide {
    pub fn empty(slots_count: usize, explanation: Explanation) -> Self {
        TradingContextBySide {
            max_amount: dec!(0),
            estimating: (0..slots_count)
                .map(|_| WithExplanation {
                    value: None,
                    explanation: explanation.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct TradingContext {
    pub by_side: EnumMap<OrderSide, TradingContextBySide>,
}

impl TradingContext {
    pub fn new(buy_ctx: TradingContextBySide, sell_ctx: TradingContextBySide) -> Self {
        TradingContext {
            // TODO use more typesafe way when it will be available for non-Copy types
            by_side: EnumMap::from_array([buy_ctx, sell_ctx]),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct PriceSlotId {
    pub strategy_name: String,
    pub level_index: usize,
}

impl PriceSlotId {
    pub fn new(strategy_name: String, level_index: usize) -> Self {
        PriceSlotId {
            strategy_name,
            level_index,
        }
    }
}

impl Display for PriceSlotId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.strategy_name, self.level_index)
    }
}

#[derive(Debug)]
pub struct OrderRecord {
    pub order: OrderRef,
    pub is_cancellation_requested: bool,
    pub request_group_id: RequestGroupId,
}

impl OrderRecord {
    fn new(order: OrderRef, request_group_id: RequestGroupId) -> Self {
        OrderRecord {
            order,
            is_cancellation_requested: false,
            request_group_id,
        }
    }
}

#[derive(Debug)]
pub struct CompositeOrder {
    pub orders: HashMap<ClientOrderId, OrderRecord>,

    /// Last estimated target price
    pub price: Decimal,

    pub side: OrderSide,
}

impl CompositeOrder {
    pub fn new(side: OrderSide) -> Self {
        CompositeOrder {
            side,
            price: dec!(0),
            orders: Default::default(),
        }
    }

    pub fn remaining_amount(&self) -> Decimal {
        self.orders
            .iter()
            .filter_map(|(_, or)| {
                let order = &or.order;
                if !order.is_finished() {
                    Some(order.fn_ref(|x| x.header.amount - x.fills.filled_amount))
                } else {
                    None
                }
            })
            .sum()
    }

    pub fn add_order_record(&mut self, order: OrderRef, request_group_id: RequestGroupId) {
        let client_order_id = order.client_order_id();
        info!(
            "Adding order clientOrderId {} in current state of DispositionExecutor",
            client_order_id
        );

        if let Some(order) = self
            .orders
            .insert(client_order_id, OrderRecord::new(order, request_group_id))
        {
            error!("The order with clientOrderId {} already exists in CompositeOrder of DispositionExecutor state when adding order record", order.order.client_order_id())
        }
    }

    pub fn remove_order(&mut self, order: &OrderRef) {
        let client_order_id = order.client_order_id();
        match self.orders.remove(&client_order_id) {
            None => error!(
                "Can't find order {} for removing in CompositeOrder of DispositionExecutor state",
                client_order_id
            ),
            Some(_) => info!(
                "Removed order {} in state of DispositionExecutor",
                client_order_id
            ),
        }
    }
}

#[derive(Debug)]
pub struct PriceSlot {
    pub id: PriceSlotId,
    pub estimating: RefCell<Option<Box<TradeCycle>>>,
    pub order: RefCell<CompositeOrder>,
}

impl PriceSlot {
    fn new(id: PriceSlotId, side: OrderSide) -> Self {
        PriceSlot {
            id,
            estimating: RefCell::new(None),
            order: RefCell::new(CompositeOrder::new(side)),
        }
    }

    fn contains(&self, order: &OrderRef) -> bool {
        self.order
            .borrow()
            .orders
            .contains_key(&order.client_order_id())
    }

    fn remove_order(&self, order: &OrderRef) {
        self.order.borrow_mut().remove_order(order)
    }

    fn add_order(
        &self,
        side: OrderSide,
        price: Price,
        order: OrderRef,
        requests_group_id: RequestGroupId,
    ) {
        let composite_order = &mut self.order.borrow_mut();
        composite_order.side = side;
        composite_order.price = price;
        composite_order.add_order_record(order, requests_group_id);
    }
}

#[derive(Debug, Getters)]
struct OrdersStateBySide {
    side: OrderSide,
    slots: Vec<PriceSlot>,
}

impl OrdersStateBySide {
    pub fn new(side: OrderSide) -> Self {
        OrdersStateBySide {
            side,
            // TODO create list of PriceSlots by config
            slots: vec![PriceSlot::new(
                PriceSlotId::new("PriceSlotId".into(), 0),
                side,
            )],
        }
    }

    pub fn calc_total_remaining_amount(&self) -> Decimal {
        self.slots
            .iter()
            .map(|x| x.order.borrow().remaining_amount())
            .sum()
    }

    pub fn traverse_price_slots(&self) -> impl Iterator<Item = &PriceSlot> {
        self.slots.iter()
    }

    pub(crate) fn find_price_slot(&self, order: &OrderRef) -> Option<&PriceSlot> {
        self.traverse_price_slots().find(|x| x.contains(order))
    }
}

#[derive(Debug)]
struct OrdersState {
    by_side: EnumMap<OrderSide, OrdersStateBySide>,
}

impl OrdersState {
    pub fn new() -> Self {
        OrdersState {
            by_side: enum_map! {
                side => OrdersStateBySide::new(side),
            },
        }
    }
}
