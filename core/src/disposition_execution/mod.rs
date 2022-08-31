pub mod executor;
pub mod trade_limit;
mod trading_context_calculation;

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use enum_map::{enum_map, EnumMap};
use itertools::Itertools;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::exchanges::common::{
    Amount, CurrencyPair, ExchangeAccountId, ExchangeId, MarketAccountId, MarketId, Price,
};
use crate::exchanges::timeouts::requests_timeout_manager::RequestGroupId;
use crate::explanation::{Explanation, ExplanationSet, PriceLevelExplanation, WithExplanation};
use crate::orders::order::{ClientOrderId, OrderRole, OrderSide};
use crate::orders::pool::OrderRef;

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
        market_account_id: MarketAccountId,
        side: OrderSide,
        price: Price,
        amount: Amount,
    ) -> Self {
        TradeDisposition {
            direction: TradeDirection {
                exchange_account_id: market_account_id.exchange_account_id,
                currency_pair: market_account_id.currency_pair,
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

    pub fn market_id(&self) -> MarketId {
        let direction = &self.direction;
        MarketId::new(
            self.direction.exchange_account_id.exchange_id,
            direction.currency_pair,
        )
    }

    pub fn market_account_id(&self) -> MarketAccountId {
        let direction = &self.direction;
        MarketAccountId::new(self.direction.exchange_account_id, direction.currency_pair)
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

    pub(crate) fn get_explanations(
        &self,
        exchange_id: ExchangeId,
        currency_pair: CurrencyPair,
    ) -> ExplanationSet {
        let explanations = self
            .by_side
            .as_slice()
            .iter()
            .flat_map(|x| x.estimating.iter().map(to_price_level_explanation))
            .collect_vec();

        ExplanationSet::new(exchange_id, currency_pair, explanations)
    }
}

fn to_price_level_explanation(
    explanation: &WithExplanation<Option<TradeCycle>>,
) -> PriceLevelExplanation {
    let SmallOrder { price, amount } = explanation
        .value
        .as_ref()
        .map(|x| x.disposition.order)
        .unwrap_or_else(|| SmallOrder::new(dec!(0), dec!(0)));

    PriceLevelExplanation {
        mode_name: "Disposition".to_string(),
        price,
        amount,
        reasons: explanation.explanation.get_reasons(),
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
        log::info!(
            "Adding order clientOrderId {} in current state of DispositionExecutor",
            client_order_id
        );

        if let Some(order) = self
            .orders
            .insert(client_order_id, OrderRecord::new(order, request_group_id))
        {
            log::error!("The order with clientOrderId {} already exists in CompositeOrder of DispositionExecutor state when adding order record", order.order.client_order_id())
        }
    }

    pub fn remove_order(&mut self, order: &OrderRef) {
        let client_order_id = order.client_order_id();
        match self.orders.remove(&client_order_id) {
            None => log::error!(
                "Can't find order {} for removing in CompositeOrder of DispositionExecutor state",
                client_order_id
            ),
            Some(_) => log::info!(
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

#[derive(Debug)]
struct OrdersStateBySide {
    _side: OrderSide,
    slots: Vec<PriceSlot>,
}

impl OrdersStateBySide {
    pub fn new(_side: OrderSide) -> Self {
        OrdersStateBySide {
            _side,
            // TODO create list of PriceSlots by config
            slots: vec![PriceSlot::new(
                PriceSlotId::new("PriceSlotId".into(), 0),
                _side,
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
