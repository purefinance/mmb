use std::any::Any;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::vec::Vec;

use crate::market::CurrencyPair;
use chrono::Utc;
use dyn_clone::{clone_trait_object, DynClone};
use enum_map::Enum;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::DateTime;
use mmb_utils::{impl_str_id, impl_u64_id, time::get_atomic_current_secs};
use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::market::{ExchangeAccountId, ExchangeErrorType, MarketAccountId, MarketId};
use crate::order::fill::{EventSourceType, OrderFill};

pub type SortedOrderData = BTreeMap<Price, Amount>;

pub type Price = Decimal;
pub type Amount = Decimal;
pub type String16 = SmallString<[u8; 16]>;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash, Enum)]
pub enum OrderSide {
    Buy = 1,
    Sell = 2,
}

impl OrderSide {
    pub fn change_side(&self) -> OrderSide {
        match self {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSide::Buy => "Buy",
            OrderSide::Sell => "Sell",
        }
    }
}

impl Display for OrderSide {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub trait OptionOrderSideExt {
    fn change_side_opt(&self) -> Option<OrderSide>;
}

impl OptionOrderSideExt for Option<OrderSide> {
    fn change_side_opt(&self) -> Option<OrderSide> {
        match self {
            None => None,
            Some(OrderSide::Buy) => Some(OrderSide::Sell),
            Some(OrderSide::Sell) => Some(OrderSide::Buy),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum OrderRole {
    Maker = 1,
    Taker = 2,
}

impl From<OrderFillRole> for OrderRole {
    fn from(fill_role: OrderFillRole) -> Self {
        match fill_role {
            OrderFillRole::Maker => OrderRole::Maker,
            OrderFillRole::Taker => OrderRole::Taker,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum OrderType {
    Unknown = 0,
    Limit = 1,
    Market = 2,
    StopLoss = 3,
    TrailingStop = 4,
    Liquidation = 5,
    ClosePosition = 6,
    MissedFill = 7,
}

impl OrderType {
    pub fn is_external_order(&self) -> bool {
        use OrderType::*;
        matches!(*self, Liquidation | ClosePosition | MissedFill)
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum OrderExecutionType {
    None = 0,
    MakerOnly = 1,
}

impl_str_id!(ClientOrderId);
impl_str_id!(ClientOrderFillId);
impl_str_id!(ExchangeOrderId);

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum OrderStatus {
    Creating = 1,
    Created = 2,
    FailedToCreate = 3,
    Canceling = 4,
    Canceled = 5,
    FailedToCancel = 6,
    Completed = 7,
}

impl Default for OrderStatus {
    fn default() -> Self {
        OrderStatus::Creating
    }
}

impl OrderStatus {
    pub fn is_finished(&self) -> bool {
        use OrderStatus::*;
        matches!(*self, FailedToCreate | Canceled | Completed)
    }
}

// Id for reserved amount
impl_u64_id!(ReservationId);

pub const CURRENT_ORDER_VERSION: u32 = 1;

/// Immutable part of order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderHeader {
    pub client_order_id: ClientOrderId,

    pub exchange_account_id: ExchangeAccountId,

    // For ClosePosition order currency pair can be empty string
    pub currency_pair: CurrencyPair,

    pub order_type: OrderType,

    pub side: OrderSide,
    pub amount: Amount,

    pub execution_type: OrderExecutionType,

    pub reservation_id: Option<ReservationId>,

    pub signal_id: Option<String>,
    pub strategy_name: String,
}

impl OrderHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client_order_id: ClientOrderId,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        order_type: OrderType,
        side: OrderSide,
        amount: Amount,
        execution_type: OrderExecutionType,
        reservation_id: Option<ReservationId>,
        signal_id: Option<String>,
        strategy_name: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            client_order_id,
            exchange_account_id,
            currency_pair,
            order_type,
            side,
            amount,
            execution_type,
            reservation_id,
            signal_id,
            strategy_name,
        })
    }

    pub fn market_account_id(&self) -> MarketAccountId {
        MarketAccountId {
            exchange_account_id: self.exchange_account_id,
            currency_pair: self.currency_pair,
        }
    }

    pub fn market_id(&self) -> MarketId {
        MarketId {
            exchange_id: self.exchange_account_id.exchange_id,
            currency_pair: self.currency_pair,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSimpleProps {
    pub init_time: DateTime,
    pub raw_price: Option<Price>,
    pub role: Option<OrderRole>,
    pub exchange_order_id: Option<ExchangeOrderId>,
    pub stop_loss_price: Decimal,
    pub trailing_stop_delta: Decimal,

    pub status: OrderStatus,

    pub finished_time: Option<DateTime>,
}

impl OrderSimpleProps {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        init_time: DateTime,
        raw_price: Option<Price>,
        role: Option<OrderRole>,
        exchange_order_id: Option<ExchangeOrderId>,
        stop_loss_price: Decimal,
        trailing_stop_delta: Decimal,
        status: OrderStatus,
        finished_time: Option<DateTime>,
    ) -> Self {
        Self {
            init_time,
            raw_price,
            role,
            exchange_order_id,
            stop_loss_price,
            trailing_stop_delta,
            status,
            finished_time,
        }
    }

    pub fn from_init_time_and_price(init_time: DateTime, price: Option<Price>) -> OrderSimpleProps {
        Self {
            init_time,
            raw_price: price,
            role: None,
            exchange_order_id: None,
            stop_loss_price: Default::default(),
            trailing_stop_delta: Default::default(),
            status: Default::default(),
            finished_time: None,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.status.is_finished()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Hash)]
pub enum OrderFillRole {
    Maker = 1,
    Taker = 2,
}

impl From<OrderRole> for OrderFillRole {
    fn from(role: OrderRole) -> Self {
        match role {
            OrderRole::Maker => OrderFillRole::Maker,
            OrderRole::Taker => OrderFillRole::Taker,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OrderFills {
    pub fills: Vec<OrderFill>,
    pub filled_amount: Decimal,
}

impl OrderFills {
    pub fn last_fill_received_time(&self) -> Option<DateTime> {
        self.fills.last().map(|x| x.receive_time())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatusChange {
    id: Uuid,
    status: OrderStatus,
    time: DateTime,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OrderStatusHistory {
    status_changes: Vec<OrderStatusChange>,
}

/// Helping properties for trading engine internal use
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SystemInternalOrderProps {
    pub creation_event_source_type: Option<EventSourceType>,
    pub last_order_creation_status_request_time: Option<DateTime>,
    pub last_creation_error_type: Option<ExchangeErrorType>,
    pub last_creation_error_message: String,

    pub cancellation_event_source_type: Option<EventSourceType>,
    pub last_order_cancellation_status_request_time: Option<DateTime>,
    pub last_cancellation_error: Option<ExchangeErrorType>,

    #[serde(skip_serializing)]
    pub is_canceling_from_wait_cancel_order: bool,

    #[serde(skip_serializing)]
    pub canceled_not_from_wait_cancel_order: bool,

    #[serde(skip_serializing)]
    pub was_cancellation_event_raised: bool,

    pub last_order_trades_request_time: Option<DateTime>,

    pub handled_by_balance_recovery: bool,
    pub filled_amount_after_cancellation: Option<Amount>,
}

/// It may be necessary for an exchange to store specific information for an order.
/// To do this, you need to implement `OrderInfoExtensionData` trait for the structure specific to the exchange.
/// For the correct implementation of the trait for `Serialize/Deserialize`, it is also necessary to add a procedural macro `#[typetag::serde]`
///
/// # Examples
///
/// ```
/// use serde::{Deserialize, Serialize};
/// use mmb_domain::order::snapshot::OrderInfoExtensionData;
/// use std::any::Any;
///
/// // Structure for which extension data is added
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct OrderInfo {
///     pub price: i32,
///     pub amount: i32,
///     pub extension_data: Box<dyn OrderInfoExtensionData>  
/// }
///
/// // Specific extension data
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// struct OrderExtensionData {
///     pub owner: String,
/// }
///
/// #[typetag::serde]
/// impl OrderInfoExtensionData for OrderExtensionData {
///     fn as_any(&self) -> &dyn Any {
///         self
///     }
///
///     fn as_mut_any(&mut self) -> &mut dyn Any {
///         self
///     }
/// }
///
/// // Creation
/// let order_info = OrderInfo {
///     price: 10,
///     amount: 2,
///     extension_data: Box::new(OrderExtensionData {
///         owner: "this".into()
///     }),
/// };
///
/// // Downcasting
/// let extension_data = order_info.extension_data
///                         .as_any()
///                         .downcast_ref::<OrderExtensionData>()
///                         .unwrap();
/// ```
#[typetag::serde(tag = "type")]
pub trait OrderInfoExtensionData: Any + DynClone + Send + Sync + Debug {
    /// Needed to call the `downcast_ref` method
    fn as_any(&self) -> &dyn Any;

    fn as_mut_any(&mut self) -> &mut dyn Any;
}

clone_trait_object!(OrderInfoExtensionData);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub currency_pair: CurrencyPair,
    pub exchange_order_id: ExchangeOrderId,
    pub client_order_id: ClientOrderId,
    pub order_side: OrderSide,
    pub order_status: OrderStatus,
    pub price: Price,
    pub amount: Amount,
    pub average_fill_price: Decimal,
    pub filled_amount: Decimal,
    pub commission_currency_code: Option<String>,
    pub commission_rate: Option<Price>,
    pub commission_amount: Option<Amount>,
    pub extension_data: Option<Box<dyn OrderInfoExtensionData>>,
}

impl OrderInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        currency_pair: CurrencyPair,
        exchange_order_id: ExchangeOrderId,
        client_order_id: ClientOrderId,
        order_side: OrderSide,
        order_status: OrderStatus,
        price: Price,
        amount: Amount,
        average_fill_price: Decimal,
        filled_amount: Decimal,
        commission_currency_code: Option<String>,
        commission_rate: Option<Price>,
        commission_amount: Option<Amount>,
    ) -> Self {
        Self {
            currency_pair,
            exchange_order_id,
            client_order_id,
            order_side,
            order_status,
            price,
            amount,
            average_fill_price,
            filled_amount,
            commission_currency_code,
            commission_rate,
            commission_amount,
            extension_data: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCreating {
    pub header: Arc<OrderHeader>,
    pub price: Price,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCancelling {
    pub header: Arc<OrderHeader>,
    pub exchange_order_id: ExchangeOrderId,
    pub extension_data: Option<Box<dyn OrderInfoExtensionData>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSnapshot {
    pub header: Arc<OrderHeader>,
    pub props: OrderSimpleProps,
    pub fills: OrderFills,
    pub status_history: OrderStatusHistory,
    pub internal_props: SystemInternalOrderProps,
    pub extension_data: Option<Box<dyn OrderInfoExtensionData>>,
}

impl OrderSnapshot {
    pub fn side(&self) -> OrderSide {
        self.header.side
    }
}

impl OrderSnapshot {
    pub fn new(
        header: Arc<OrderHeader>,
        props: OrderSimpleProps,
        fills: OrderFills,
        status_history: OrderStatusHistory,
        internal_props: SystemInternalOrderProps,
        extension_data: Option<Box<dyn OrderInfoExtensionData>>,
    ) -> Self {
        OrderSnapshot {
            header,
            props,
            fills,
            status_history,
            internal_props,
            extension_data,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_params(
        client_order_id: ClientOrderId,
        order_type: OrderType,
        order_role: Option<OrderRole>,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        price: Price,
        amount: Amount,
        order_side: OrderSide,
        reservation_id: Option<ReservationId>,
        strategy_name: &str,
    ) -> Self {
        let header = OrderHeader::new(
            client_order_id,
            exchange_account_id,
            currency_pair,
            order_type,
            order_side,
            amount,
            OrderExecutionType::None,
            reservation_id,
            None,
            strategy_name.to_owned(),
        );

        let mut props = OrderSimpleProps::from_init_time_and_price(Utc::now(), Some(price));
        props.role = order_role;

        Self::new(
            header,
            props,
            OrderFills::default(),
            OrderStatusHistory::default(),
            SystemInternalOrderProps::default(),
            None,
        )
    }

    pub fn add_fill(&mut self, fill: OrderFill) {
        self.fills.filled_amount += fill.amount();
        self.fills.fills.push(fill);
    }

    pub fn status(&self) -> OrderStatus {
        self.props.status
    }

    pub fn set_status(&mut self, new_status: OrderStatus, time: DateTime) {
        self.props.status = new_status;
        if new_status.is_finished() {
            self.props.finished_time = Some(time);
        }
        self.status_history.status_changes.push(OrderStatusChange {
            id: Uuid::default(),
            status: new_status,
            time,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.props.is_finished()
    }

    pub fn price(&self) -> Price {
        self.props.raw_price.with_expect(|| {
            let client_order_id = self.header.client_order_id.as_str();
            format!("Cannot get price from order {client_order_id}")
        })
    }

    pub fn amount(&self) -> Amount {
        self.header.amount
    }
    pub fn filled_amount(&self) -> Amount {
        self.fills.filled_amount
    }

    pub fn market_account_id(&self) -> MarketAccountId {
        self.header.market_account_id()
    }

    pub fn market_id(&self) -> MarketId {
        self.header.market_id()
    }

    pub fn client_order_id(&self) -> ClientOrderId {
        self.header.client_order_id.clone()
    }

    pub fn exchange_order_id(&self) -> Option<ExchangeOrderId> {
        self.props.exchange_order_id.clone()
    }

    pub fn currency_pair(&self) -> CurrencyPair {
        self.header.currency_pair
    }

    pub fn init_time(&self) -> DateTime {
        self.props.init_time
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PriceByOrderSide {
    pub top_bid: Option<Price>,
    pub top_ask: Option<Price>,
}

impl PriceByOrderSide {
    pub fn new(top_bid: Option<Price>, top_ask: Option<Price>) -> Self {
        Self { top_bid, top_ask }
    }
}
