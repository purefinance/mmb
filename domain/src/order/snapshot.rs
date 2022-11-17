use crate::events::EventSourceType;
use crate::market::CurrencyPair;
use crate::market::{ExchangeAccountId, ExchangeErrorType, MarketAccountId, MarketId};
use crate::order::fill::OrderFill;
use chrono::Utc;
use dyn_clone::{clone_trait_object, DynClone};
use enum_map::Enum;
use mmb_database::impl_event;
use mmb_utils::{impl_from_for_str_id, DateTime};
use mmb_utils::{impl_str_id, impl_u64_id, time::get_atomic_current_secs};
use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smallstr::SmallString;
use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::vec::Vec;
use uuid::Uuid;

pub type SortedOrderData = BTreeMap<Price, Amount>;

pub type Price = Decimal;
/// Amount is a currency quantity alias: order amount, position amount, balance amount, etc.
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

impl_from_for_str_id!(i64, ClientOrderId);
impl_from_for_str_id!(u64, ClientOrderId);
impl_from_for_str_id!(i32, ClientOrderId);

impl From<&i32> for ClientOrderId {
    fn from(value: &i32) -> Self {
        Self::from(*value)
    }
}

impl_str_id!(ClientOrderFillId);
impl_str_id!(ExchangeOrderId);

impl_from_for_str_id!(i64, ExchangeOrderId);
impl_from_for_str_id!(u64, ExchangeOrderId);
impl_from_for_str_id!(i32, ExchangeOrderId);

impl From<&i32> for ExchangeOrderId {
    fn from(value: &i32) -> Self {
        Self::from(*value)
    }
}

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

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum UserOrder {
    // Create order with specified price or make taker order if market was crossed with specified price
    Limit {
        price: Price,
        execution_type: OrderExecutionType,
    },
    /// Immediately trade taker order by another order side price
    Market,
    /// Create market order when triggered stop-loss price
    StopLoss {
        /// Price for stop-loss order trigger
        stop_price: Price,
    },
    TrailingStop {
        trailing_delta: Decimal,
        stop_price: Option<Price>,
    },
}

impl UserOrder {
    /// Limit order (not maker only)    
    pub fn limit(price: Price) -> Self {
        Self::Limit {
            price,
            execution_type: OrderExecutionType::None,
        }
    }

    /// Limit maker only order
    pub fn maker_only(price: Price) -> Self {
        Self::Limit {
            price,
            execution_type: OrderExecutionType::MakerOnly,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExternalOrder {
    Liquidation { price: Price },
    ClosePosition { price: Price },
    MissedFill { price: Price },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderOptions {
    Unknown { price: Option<Price> },
    User(UserOrder),
    External(ExternalOrder),
}

impl OrderOptions {
    /// Limit order (not maker only)    
    pub fn limit(price: Price) -> Self {
        Self::User(UserOrder::limit(price))
    }

    /// Limit maker only order
    pub fn maker_only(price: Price) -> Self {
        Self::User(UserOrder::maker_only(price))
    }

    pub fn unknown(price: Option<Price>) -> Self {
        Self::Unknown { price }
    }

    pub fn liquidation(price: Price) -> Self {
        Self::External(ExternalOrder::Liquidation { price })
    }

    pub fn close_position(price: Price) -> Self {
        Self::External(ExternalOrder::ClosePosition { price })
    }

    pub(crate) fn get_source_price(&self) -> Option<Price> {
        match self {
            OrderOptions::User(UserOrder::Limit { price, .. })
            | OrderOptions::External(ExternalOrder::Liquidation { price })
            | OrderOptions::External(ExternalOrder::ClosePosition { price })
            | OrderOptions::External(ExternalOrder::MissedFill { price }) => Some(*price),
            OrderOptions::Unknown { price } => *price,
            _ => None,
        }
    }

    pub fn get_order_type(&self) -> OrderType {
        match self {
            OrderOptions::Unknown { .. } => OrderType::Unknown,
            OrderOptions::User(UserOrder::Limit { .. }) => OrderType::Limit,
            OrderOptions::User(UserOrder::Market { .. }) => OrderType::Market,
            OrderOptions::User(UserOrder::StopLoss { .. }) => OrderType::StopLoss,
            OrderOptions::User(UserOrder::TrailingStop { .. }) => OrderType::TrailingStop,
            OrderOptions::External(ExternalOrder::Liquidation { .. }) => OrderType::Liquidation,
            OrderOptions::External(ExternalOrder::ClosePosition { .. }) => OrderType::ClosePosition,
            OrderOptions::External(ExternalOrder::MissedFill { .. }) => OrderType::MissedFill,
        }
    }

    pub fn execution_type(&self) -> Option<OrderExecutionType> {
        match self {
            OrderOptions::User(UserOrder::Limit { execution_type, .. }) => Some(*execution_type),
            _ => None,
        }
    }
}

/// Immutable part of order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderHeader {
    pub client_order_id: ClientOrderId,
    pub exchange_account_id: ExchangeAccountId,

    // NOTE: For ClosePosition order currency pair can be empty string
    pub currency_pair: CurrencyPair,

    pub side: OrderSide,
    pub amount: Amount,

    pub options: OrderOptions,

    /// Price of order specified by exchange client before order creation.
    /// Price should be specified for `Limit` order and should not be specified for `Market` order.
    /// For other order types it depends on exchange requirements.
    pub source_price: Option<Price>,
    pub order_type: OrderType,

    pub reservation_id: Option<ReservationId>,

    pub signal_id: Option<String>,
    pub strategy_name: String,
}

impl OrderHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn with_user_order(
        client_order_id: ClientOrderId,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        side: OrderSide,
        amount: Amount,
        user_order: UserOrder,
        reservation_id: Option<ReservationId>,
        signal_id: Option<String>,
        strategy_name: String,
    ) -> Self {
        Self::with_options(
            client_order_id,
            exchange_account_id,
            currency_pair,
            side,
            amount,
            OrderOptions::User(user_order),
            reservation_id,
            signal_id,
            strategy_name,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        client_order_id: ClientOrderId,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        side: OrderSide,
        amount: Amount,
        options: OrderOptions,
        reservation_id: Option<ReservationId>,
        signal_id: Option<String>,
        strategy_name: String,
    ) -> Self {
        Self {
            client_order_id,
            exchange_account_id,
            currency_pair,
            order_type: options.get_order_type(),
            source_price: options.get_source_price(),
            side,
            amount,
            options,
            reservation_id,
            signal_id,
            strategy_name,
        }
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

    /// NOTE: Should be used only in cases when we sure that price specified
    pub fn price(&self) -> Price {
        self.source_price
            .unwrap_or_else(|| panic!("Cannot get price from order {}", self.client_order_id))
    }

    /// Price of order specified by exchange client before order creation.
    /// Price should be specified for `Limit` order and should not be specified for `Market` order.
    /// For other order types it depends on exchange requirements.
    pub fn source_price(&self) -> Option<Price> {
        self.source_price
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSimpleProps {
    pub init_time: DateTime,
    pub exchange_order_id: Option<ExchangeOrderId>,

    pub status: OrderStatus,

    pub role: Option<OrderRole>,
    pub finished_time: Option<DateTime>,
}

impl OrderSimpleProps {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        init_time: DateTime,
        role: Option<OrderRole>,
        exchange_order_id: Option<ExchangeOrderId>,
        status: OrderStatus,
        finished_time: Option<DateTime>,
    ) -> Self {
        Self {
            init_time,
            role,
            exchange_order_id,
            status,
            finished_time,
        }
    }

    pub fn from_init_time(init_time: DateTime) -> OrderSimpleProps {
        Self {
            init_time,
            role: None,
            exchange_order_id: None,
            status: OrderStatus::default(),
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
    pub filled_amount: Amount,
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

// In some cases exchange doesn't send price, amount, average_fill_price and filled_amount values.
// So it will be 0
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

/// Mutable part of order
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OrderMut {
    pub props: OrderSimpleProps,
    pub fills: OrderFills,
    pub status_history: OrderStatusHistory,
    pub internal_props: SystemInternalOrderProps,
    pub extension_data: Option<Box<dyn OrderInfoExtensionData>>,
}

impl OrderMut {
    pub fn add_fill(&mut self, fill: OrderFill) {
        self.fills.filled_amount += fill.amount();
        self.fills.fills.push(fill);
    }

    pub fn status(&self) -> OrderStatus {
        self.props.status
    }

    pub fn set_status(&mut self, new_status: OrderStatus, time: DateTime) {
        set_status(&mut self.props, &mut self.status_history, new_status, time);
    }

    pub fn is_finished(&self) -> bool {
        self.props.is_finished()
    }

    pub fn filled_amount(&self) -> Amount {
        self.fills.filled_amount
    }

    pub fn exchange_order_id(&self) -> Option<ExchangeOrderId> {
        self.props.exchange_order_id.clone()
    }

    pub fn init_time(&self) -> DateTime {
        self.props.init_time
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSnapshot {
    pub header: OrderHeader,
    pub props: OrderSimpleProps,
    pub fills: OrderFills,
    pub status_history: OrderStatusHistory,
    pub internal_props: SystemInternalOrderProps,
    pub extension_data: Option<Box<dyn OrderInfoExtensionData>>,
}

impl_event!(&mut OrderSnapshot, "orders");

impl OrderSnapshot {
    pub fn new(
        header: OrderHeader,
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
        options: OrderOptions,
        order_role: Option<OrderRole>,
        exchange_account_id: ExchangeAccountId,
        currency_pair: CurrencyPair,
        amount: Amount,
        order_side: OrderSide,
        reservation_id: Option<ReservationId>,
        strategy_name: &str,
    ) -> Self {
        let header = OrderHeader::with_options(
            client_order_id,
            exchange_account_id,
            currency_pair,
            order_side,
            amount,
            options,
            reservation_id,
            None,
            strategy_name.to_owned(),
        );

        let mut props = OrderSimpleProps::from_init_time(Utc::now());
        props.role = order_role;

        Self {
            header,
            props,
            fills: OrderFills::default(),
            status_history: OrderStatusHistory::default(),
            internal_props: SystemInternalOrderProps::default(),
            extension_data: None,
        }
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

    pub fn currency_pair(&self) -> CurrencyPair {
        self.header.currency_pair
    }

    pub fn side(&self) -> OrderSide {
        self.header.side
    }

    /// NOTE: Should be used only in cases when we sure that price specified
    pub fn price(&self) -> Price {
        self.header
            .source_price
            .unwrap_or_else(|| panic!("Cannot get price from order {}", self.client_order_id()))
    }

    pub fn amount(&self) -> Amount {
        self.header.amount
    }

    pub fn status(&self) -> OrderStatus {
        self.props.status
    }

    pub fn add_fill(&mut self, fill: OrderFill) {
        self.fills.filled_amount += fill.amount();
        self.fills.fills.push(fill);
    }

    pub fn set_status(&mut self, new_status: OrderStatus, time: DateTime) {
        set_status(&mut self.props, &mut self.status_history, new_status, time);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceByOrderSide {
    pub top_bid: Option<Price>,
    pub top_ask: Option<Price>,
}

impl PriceByOrderSide {
    pub fn new(top_bid: Option<Price>, top_ask: Option<Price>) -> Self {
        Self { top_bid, top_ask }
    }
}

impl Display for PriceByOrderSide {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ask {:?}, bid {:?}", self.top_ask, self.top_bid)
    }
}

fn set_status(
    props: &mut OrderSimpleProps,
    status_history: &mut OrderStatusHistory,
    new_status: OrderStatus,
    time: DateTime,
) {
    props.status = new_status;
    if new_status.is_finished() {
        props.finished_time = Some(time);
    }
    status_history.status_changes.push(OrderStatusChange {
        id: Uuid::default(),
        status: new_status,
        time,
    })
}
