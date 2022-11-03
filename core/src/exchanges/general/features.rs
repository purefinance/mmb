use mmb_domain::events::AllowedEventSourceType;

#[derive(Debug)]
pub enum OpenOrdersType {
    // TODO None is redundant type, used only for error print and make OneCurrencyPair as default
    None,
    AllCurrencyPair,
    /// Some exchanges does not allow to get all open orders
    /// So we should extract orders for each currency pair
    OneCurrencyPair,
}

#[derive(Debug)]
pub enum RestFillsType {
    /// Uninitialized, should cause panic when it's used
    None,
    /// Get fills from trades
    MyTrades,
    /// Get fills from order info
    GetOrderInfo,
}

impl Default for RestFillsType {
    fn default() -> Self {
        RestFillsType::None
    }
}

#[derive(Default, Debug)]
pub struct RestFillsFeatures {
    pub fills_type: RestFillsType,
}

impl RestFillsFeatures {
    pub fn new(fills_type: RestFillsType) -> Self {
        Self { fills_type }
    }
}
#[derive(Default)]
pub struct WebSocketOptions {
    /// Is order execution result able to receive
    pub execution_notification: bool,
    /// Is order cancellation result able to receive
    pub cancellation_notification: bool,
    // TODO Not used, is it redundant?
    pub supports_ping_pong: bool,
    // TODO Used in exchange inner not in core, is it redundant?
    pub supports_subscription_response: bool,
}

impl WebSocketOptions {
    pub fn new(
        execution_notification: bool,
        cancellation_notification: bool,
        supports_ping_pong: bool,
        supports_subscription_response: bool,
    ) -> Self {
        Self {
            execution_notification,
            cancellation_notification,
            supports_ping_pong,
            supports_subscription_response,
        }
    }
}

#[derive(Default)]
pub struct OrderFeatures {
    /// Maker only orders are supported
    // TODO Refactor name to be self-documenting
    pub maker_only: bool,
    pub supports_get_order_info_by_client_order_id: bool,
    /// On some exchanges cancellation rest fallback doesn't mean an order was canceled, it just signals an exchange has started the cancellation process
    pub cancellation_response_from_rest_only_for_errors: bool,
    /// On some exchanges creation rest fallback doesn't mean an order was created, it just signals an exchange has started the creation process
    pub creation_response_from_rest_only_for_errors: bool,
    // Flag is used only in couple of tests
    // TODO Possible remove it
    pub order_was_completed_error_for_cancellation: bool,
    // Flag is used only in one test
    // TODO Possible remove it
    pub supports_already_cancelled_order: bool,
    /// Stop loss orders are supported
    // TODO Flag is not used in core, is it redundant?
    pub supports_stop_loss_order: bool,
}

impl OrderFeatures {
    pub fn new(
        maker_only: bool,
        supports_get_order_info_by_client_order_id: bool,
        cancellation_response_from_rest_only_for_errors: bool,
        creation_response_from_rest_only_for_errors: bool,
        order_was_completed_error_for_cancellation: bool,
        supports_already_cancelled_order: bool,
        supports_stop_loss_order: bool,
    ) -> Self {
        Self {
            maker_only,
            supports_get_order_info_by_client_order_id,
            cancellation_response_from_rest_only_for_errors,
            creation_response_from_rest_only_for_errors,
            order_was_completed_error_for_cancellation,
            supports_already_cancelled_order,
            supports_stop_loss_order,
        }
    }
}

#[derive(Default)]
pub struct OrderTradeOption {
    /// Get trades result contain timestamp
    // TODO Used only in tests, is it redundant?
    pub supports_trade_time: bool,
    /// Trade id is a number not string
    pub supports_trade_incremented_id: bool,
    // TODO Refactor name
    pub supports_get_prints: bool,
    // TODO Used in single test that only checks if exchange has this flag. Is it redundant?
    pub supports_tick_direction: bool,
    // TODO Repeats supports_trade_time functional and used only in tests. Is it redundant?
    pub supports_my_trades_from_time: bool,
}

pub enum BalancePositionOption {
    NonDerivative,
    SingleRequest,
    IndividualRequests,
}

pub struct ExchangeFeatures {
    /// Exchange client possibility of getting open orders: all in single request or by each currency pair separately
    // TODO Possible redundant cause it's exchange client implementation part and core always requests all open orders
    pub open_orders_type: OpenOrdersType,
    /// A way how exchange client can get order fill info: from trades of from order
    // TODO We use only RestFillType enum from RestFillsFeatures struct, possible refactor
    pub rest_fills_features: RestFillsFeatures,
    /// Order features, core check its flags for different behavior
    pub order_features: OrderFeatures,
    /// Trades specific flags
    pub trade_option: OrderTradeOption,
    /// Websocket messages handling specific options
    pub websocket_options: WebSocketOptions,
    /// If empty content string of RestClient response is normal situation for the exchange
    pub empty_response_is_ok: bool,
    pub balance_position_option: BalancePositionOption,

    // used only for debug
    pub allowed_create_event_source_type: AllowedEventSourceType,
    // used only for debug
    pub allowed_fill_event_source_type: AllowedEventSourceType,
    // used only for debug
    pub allowed_cancel_event_source_type: AllowedEventSourceType,
}

impl ExchangeFeatures {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        open_orders_type: OpenOrdersType,
        rest_fills_features: RestFillsFeatures,
        order_features: OrderFeatures,
        trade_option: OrderTradeOption,
        websocket_options: WebSocketOptions,
        empty_response_is_ok: bool,
        allowed_create_event_source_type: AllowedEventSourceType,
        allowed_fill_event_source_type: AllowedEventSourceType,
        allowed_cancel_event_source_type: AllowedEventSourceType,
    ) -> Self {
        Self {
            open_orders_type,
            rest_fills_features,
            order_features,
            trade_option,
            websocket_options,
            empty_response_is_ok,
            allowed_create_event_source_type,
            allowed_fill_event_source_type,
            allowed_cancel_event_source_type,
            balance_position_option: BalancePositionOption::NonDerivative,
        }
    }
}
