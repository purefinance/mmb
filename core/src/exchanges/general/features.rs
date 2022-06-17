use crate::exchanges::events::AllowedEventSourceType;

#[derive(Debug)]
pub enum OpenOrdersType {
    None,
    AllCurrencyPair,
    // Some exchanges does not allow to get all open orders
    // So we should extract orders for each currency pair
    OneCurrencyPair,
}

#[derive(Debug)]
pub enum RestFillsType {
    None,
    MyTrades,
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
    // TODO all over fields for check_order_fills()
}

impl RestFillsFeatures {
    pub fn new(fills_type: RestFillsType) -> Self {
        Self { fills_type }
    }
}
#[derive(Default)]
pub struct WebSocketOptions {
    pub execution_notification: bool,
    pub cancellation_notification: bool,
    pub supports_ping_pong: bool,
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
    pub maker_only: bool,
    pub supports_get_order_info_by_client_order_id: bool,
    pub cancellation_response_from_rest_only_for_errors: bool,
    pub creation_response_from_rest_only_for_errors: bool,
    pub order_was_completed_error_for_cancellation: bool,
    pub supports_already_cancelled_order: bool,
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
    pub supports_trade_time: bool,
    pub supports_trade_incremented_id: bool,

    // At ByBit subscription to Print notification only available for all currency pairs
    pub notification_on_each_currency_pair: bool,
    pub supports_get_prints: bool,
    pub supports_tick_direction: bool,
    pub supports_my_trades_from_time: bool,
}

pub enum BalancePositionOption {
    NonDerivative,
    SingleRequest,
    IndividualRequests,
}

pub struct ExchangeFeatures {
    pub open_orders_type: OpenOrdersType,
    pub rest_fills_features: RestFillsFeatures,
    pub order_features: OrderFeatures,
    pub trade_option: OrderTradeOption,
    pub websocket_options: WebSocketOptions,
    pub empty_response_is_ok: bool,
    pub allows_to_get_order_info_by_client_order_id: bool,
    pub allowed_fill_event_source_type: AllowedEventSourceType,
    pub allowed_cancel_event_source_type: AllowedEventSourceType,
    pub balance_position_option: BalancePositionOption,
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
        allows_to_get_order_info_by_client_order_id: bool,
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
            allows_to_get_order_info_by_client_order_id,
            allowed_fill_event_source_type,
            allowed_cancel_event_source_type,
            balance_position_option: BalancePositionOption::NonDerivative,
        }
    }
}
