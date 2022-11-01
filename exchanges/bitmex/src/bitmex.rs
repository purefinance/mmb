use crate::support::BitmexOrderFill;
use crate::types::{
    BitmexBalanceInfo, BitmexOrderInfo, BitmexSymbol, BitmexSymbolType, BitmexWalletAsset,
    PositionPayload,
};
use anyhow::{anyhow, Context, Result};
use arrayvec::{ArrayString, ArrayVec};
use dashmap::DashMap;
use function_name::named;
use hmac::{Hmac, Mac};
use hyper::http::request::Builder;
use hyper::{StatusCode, Uri};
use itertools::Itertools;
use mmb_core::exchanges::general::features::{
    BalancePositionOption, ExchangeFeatures, OpenOrdersType, OrderFeatures, OrderTradeOption,
    RestFillsFeatures, RestFillsType, WebSocketOptions,
};
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::rest_client::{
    ErrorHandler, ErrorHandlerData, RequestType, RestClient, RestHeaders, RestResponse, UriBuilder,
};
use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestTimeoutArguments;
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::exchanges::traits::{
    ExchangeClientBuilder, ExchangeClientBuilderResult, ExchangeError, HandleOrderFilledCb,
    HandleTradeCb, OrderCancelledCb, OrderCreatedCb, SendWebsocketMessageCb, Support,
};
use mmb_core::lifecycle::app_lifetime_manager::AppLifetimeManager;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::events::{
    AllowedEventSourceType, ExchangeBalance, ExchangeBalancesAndPositions, ExchangeEvent,
};
use mmb_domain::exchanges::symbol::{Precision, Symbol};
use mmb_domain::market::{
    CurrencyCode, CurrencyId, CurrencyPair, ExchangeErrorType, ExchangeId, SpecificCurrencyPair,
};
use mmb_domain::order::pool::{OrderRef, OrdersPool};
use mmb_domain::order::snapshot::{
    ExchangeOrderId, OrderCancelling, OrderExecutionType, OrderInfo, OrderRole, OrderSide,
    OrderStatus, OrderType, Price,
};
use mmb_domain::position::{ActivePosition, ClosedPosition, DerivativePosition};
use mmb_utils::DateTime;
use parking_lot::{Mutex, RwLock};
use rust_decimal::Decimal;
use rust_decimal::MathematicalOps;
use rust_decimal_macros::dec;
use serde::Deserialize;
use sha2::Sha256;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tinyvec::Array;
use tokio::sync::broadcast;
use urlencoding_macro::encode;

#[derive(Default)]
pub struct ErrorHandlerBitmex;

impl ErrorHandler for ErrorHandlerBitmex {
    fn check_spec_rest_error(&self, response: &RestResponse) -> Result<(), ExchangeError> {
        match response.status {
            // StatusCode::UNAUTHORIZED is possible too but it is handling in RestClient::get_rest_error()
            StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Err(
                ExchangeError::new(ExchangeErrorType::SendError, response.content.clone(), None),
            ),
            StatusCode::OK => {
                if response.content.contains("error") {
                    Err(ExchangeError::unknown(&response.content))
                } else {
                    Ok(())
                }
            }
            _ => Err(ExchangeError::unknown(&response.content)),
        }
    }

    fn clarify_error_type(&self, error: &ExchangeError) -> ExchangeErrorType {
        if error.message.contains("Invalid orderID") {
            ExchangeErrorType::OrderNotFound
        } else if error.message.contains("Unable to cancel order") {
            ExchangeErrorType::InvalidOrder
        } else {
            ExchangeErrorType::Unknown
        }
    }
}

pub struct RestHeadersBitmex {
    api_key: String,
    secret_key: String,
}

impl RestHeadersBitmex {
    pub fn new(api_key: String, secret_key: String) -> Self {
        Self {
            api_key,
            secret_key,
        }
    }

    pub fn create_signature_message(
        path_and_query: &str,
        request_type: RequestType,
    ) -> (ArrayString<256>, u64) {
        let mut message = ArrayString::<256>::new();
        message.push_str(request_type.as_str());
        message.push_str(path_and_query);

        let expire_time = Bitmex::get_key_expire_time(60);

        (message, expire_time)
    }
}

impl RestHeaders for RestHeadersBitmex {
    fn add_specific_headers(
        &self,
        builder: Builder,
        uri: &Uri,
        request_type: RequestType,
    ) -> Builder {
        let path_and_query = match uri.path_and_query() {
            Some(path_and_query) => path_and_query.as_str(),
            None => uri.path(),
        };
        let (message, expire_time) =
            RestHeadersBitmex::create_signature_message(path_and_query, request_type);

        builder
            .header("api-expires", expire_time)
            .header("api-key", &self.api_key)
            .header(
                "api-signature",
                Bitmex::create_signature(&self.secret_key, message.as_str(), expire_time)
                    .as_slice(),
            )
    }
}

const EMPTY_RESPONSE_IS_OK: bool = false;

pub struct Bitmex {
    pub(crate) settings: ExchangeSettings,
    pub hosts: Hosts,
    rest_client: RestClient<ErrorHandlerBitmex, RestHeadersBitmex>,
    pub(crate) unified_to_specific: RwLock<HashMap<CurrencyPair, SpecificCurrencyPair>>,
    specific_to_unified: RwLock<HashMap<SpecificCurrencyPair, CurrencyPair>>,
    pub(crate) supported_currencies: DashMap<CurrencyId, CurrencyCode>,
    // Currencies used for trading according to user settings
    pub(super) traded_specific_currencies: Mutex<Vec<SpecificCurrencyPair>>,
    pub(super) lifetime_manager: Arc<AppLifetimeManager>,
    pub(super) events_channel: broadcast::Sender<ExchangeEvent>,
    pub(crate) order_created_callback: OrderCreatedCb,
    pub(crate) order_cancelled_callback: OrderCancelledCb,
    pub(crate) handle_order_filled_callback: HandleOrderFilledCb,
    pub(crate) handle_trade_callback: HandleTradeCb,
    pub(crate) websocket_message_callback: SendWebsocketMessageCb,
    pub(super) order_book_ids: Mutex<HashMap<(SpecificCurrencyPair, u64), Price>>,
    currency_balance_rates: Mutex<HashMap<CurrencyCode, Decimal>>,
}

impl Bitmex {
    pub fn new(
        settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
    ) -> Bitmex {
        Self {
            rest_client: RestClient::new(
                ErrorHandlerData::new(
                    EMPTY_RESPONSE_IS_OK,
                    settings.exchange_account_id,
                    ErrorHandlerBitmex::default(),
                ),
                RestHeadersBitmex::new(settings.api_key.clone(), settings.secret_key.clone()),
            ),
            settings,
            hosts: Self::make_hosts(),
            unified_to_specific: Default::default(),
            specific_to_unified: Default::default(),
            supported_currencies: Default::default(),
            traded_specific_currencies: Default::default(),
            events_channel,
            lifetime_manager,
            order_created_callback: Box::new(|_, _, _| {}),
            order_cancelled_callback: Box::new(|_, _, _| {}),
            handle_order_filled_callback: Box::new(|_| {}),
            handle_trade_callback: Box::new(|_, _| {}),
            websocket_message_callback: Box::new(|_, _| Ok(())),
            order_book_ids: Default::default(),
            currency_balance_rates: Default::default(),
        }
    }

    fn make_hosts() -> Hosts {
        Hosts {
            web_socket_host: "wss://www.bitmex.com/realtime",
            web_socket2_host: "wss://www.bitmex.com/realtime",
            rest_host: "https://www.bitmex.com",
        }
    }

    #[named]
    pub(super) async fn request_all_symbols(&self) -> Result<RestResponse, ExchangeError> {
        let builder = UriBuilder::from_path("/api/v1/instrument/active");
        let uri = builder.build_uri(self.hosts.rest_uri_host(), false);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_all_symbols(&self, response: &RestResponse) -> Result<Vec<Arc<Symbol>>> {
        let symbols: Vec<BitmexSymbol> = serde_json::from_str(&response.content)
            .context("Unable to deserialize response from Bitmex")?;

        Ok(symbols
            .iter()
            .filter_map(|symbol| {
                self.filter_symbol(symbol).map(|symbol| {
                    let base = symbol.base_id.into();
                    let quote = symbol.quote_id.into();

                    let specific_currency_pair = symbol.id.into();
                    let unified_currency_pair = CurrencyPair::from_codes(base, quote);
                    self.unified_to_specific
                        .write()
                        .insert(unified_currency_pair, specific_currency_pair);

                    self.specific_to_unified
                        .write()
                        .insert(specific_currency_pair, unified_currency_pair);

                    let (amount_currency_code, balance_currency_code) =
                        match self.settings.is_margin_trading {
                            true => (quote, Some(base)),
                            false => (base, None),
                        };

                    Arc::new(Symbol::new(
                        self.settings.is_margin_trading,
                        symbol.base_id.into(),
                        base,
                        symbol.quote_id.into(),
                        quote,
                        None,
                        symbol.max_price,
                        Some(symbol.amount_tick),
                        symbol.max_amount,
                        None,
                        amount_currency_code,
                        balance_currency_code,
                        Precision::ByTick {
                            tick: symbol.price_tick,
                        },
                        Precision::ByTick {
                            tick: symbol.amount_tick,
                        },
                    ))
                })
            })
            .collect_vec())
    }

    fn filter_symbol<'a>(&self, symbol: &'a BitmexSymbol<'a>) -> Option<&'a BitmexSymbol<'a>> {
        let symbol_type = BitmexSymbolType::try_from(symbol.symbol_type).ok()?;

        let is_active_symbol = symbol.state == "Open";
        let is_supported = match self.settings.is_margin_trading {
            true => symbol_type == BitmexSymbolType::PerpetualContract && symbol.id != "ETHUSD_ETH", // ETHUSD_ETH is a ETH-margined perpetual swap. We don't support it at the moment
            false => symbol_type == BitmexSymbolType::Spot,
        };

        if is_active_symbol && is_supported {
            Some(symbol)
        } else {
            None
        }
    }

    #[named]
    pub(super) async fn do_create_order(
        &self,
        order: &OrderRef,
    ) -> Result<RestResponse, ExchangeError> {
        let (header, price, stop_loss_price, mut trailing_stop_delta) = order.fn_ref(|order| {
            (
                order.header.clone(),
                order.price(),
                order.props.stop_loss_price,
                order.props.trailing_stop_delta,
            )
        });
        let specific_currency_pair = self.get_specific_currency_pair(header.currency_pair);

        let mut builder = UriBuilder::from_path("/api/v1/order");
        builder.add_kv("symbol", specific_currency_pair);
        builder.add_kv("side", header.side.as_str());
        builder.add_kv("orderQty", header.amount);
        builder.add_kv("clOrdID", header.client_order_id.as_str());

        match header.order_type {
            OrderType::Market => builder.add_kv("ordType", "Market"),
            OrderType::Limit => {
                builder.add_kv("ordType", "Limit");
                builder.add_kv("price", price);
                if header.execution_type == OrderExecutionType::MakerOnly {
                    builder.add_kv("execInst", "ParticipateDoNotInitiate");
                }
            }
            OrderType::StopLoss => {
                builder.add_kv("ordType", "Stop");
                builder.add_kv("stopPx", stop_loss_price);
            }
            OrderType::TrailingStop => {
                builder.add_kv("ordType", "Stop");
                builder.add_kv("pegPriceType", "TrailingStopPeg");
                if header.side == OrderSide::Sell {
                    trailing_stop_delta.set_sign_negative(true);
                }
                builder.add_kv("pegOffsetValue", trailing_stop_delta);
            }
            OrderType::ClosePosition => {
                // It will cancel other active limit orders with the same side and symbol if the open quantity exceeds the current position
                // Details: https://www.bitmex.com/api/explorer/#!/Order/Order_new
                builder.add_kv("ordType", "Close");
            }
            _ => return Err(ExchangeError::unknown("Unexpected order type")),
        }

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);
        let log_args = format!("Create order for {header:?}");
        self.rest_client
            .post(uri, None, function_name!(), log_args)
            .await
    }

    pub(super) fn get_order_id(
        &self,
        response: &RestResponse,
    ) -> Result<ExchangeOrderId, ExchangeError> {
        #[derive(Deserialize)]
        struct OrderId<'a> {
            #[serde(rename = "orderID")]
            order_id: &'a str,
        }

        let deserialized: OrderId = serde_json::from_str(&response.content)
            .map_err(|err| ExchangeError::parsing(format!("Unable to parse orderId: {err:?}")))?;

        Ok(ExchangeOrderId::from(deserialized.order_id))
    }

    #[named]
    pub(super) async fn request_open_orders(
        &self,
        currency_pair: Option<CurrencyPair>,
    ) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/api/v1/order");

        builder.add_kv("filter", encode!(r#"{"open": true}"#));
        if let Some(pair) = currency_pair {
            builder.add_kv("symbol", self.get_specific_currency_pair(pair));
        }

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);
        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_open_orders(&self, response: &RestResponse) -> Result<Vec<OrderInfo>> {
        let bitmex_orders: Vec<BitmexOrderInfo> = serde_json::from_str(&response.content)
            .context("Unable to parse response content for get_open_orders request")?;

        Ok(bitmex_orders
            .iter()
            .map(|order| self.specific_order_info_to_unified(order))
            .collect())
    }

    fn specific_order_info_to_unified(&self, specific: &BitmexOrderInfo) -> OrderInfo {
        let price = match specific.price {
            Some(price) => price,
            None => dec!(0),
        };
        let average_price = match specific.average_fill_price {
            Some(price) => price,
            None => dec!(0),
        };
        let amount = match specific.amount {
            Some(amount) => amount,
            None => dec!(0),
        };
        let filled_amount = match specific.filled_amount {
            Some(amount) => amount,
            None => dec!(0),
        };
        OrderInfo::new(
            self.get_unified_currency_pair(&specific.specific_currency_pair)
                .expect("Expected known currency pair"),
            specific.exchange_order_id.clone(),
            specific.client_order_id.clone(),
            specific.side,
            Bitmex::get_local_order_status(specific.status),
            price,
            amount,
            average_price,
            filled_amount,
            // Bitmex doesn't return commission info on GET /order request
            None,
            None,
            None,
        )
    }

    pub(super) fn get_unified_currency_pair(
        &self,
        currency_pair: &SpecificCurrencyPair,
    ) -> Result<CurrencyPair> {
        self.specific_to_unified
            .read()
            .get(currency_pair)
            .cloned()
            .with_context(|| {
                format!(
                    "Not found currency pair '{currency_pair:?}' in {}",
                    self.settings.exchange_account_id
                )
            })
    }

    pub(super) fn get_local_order_status(status: &str) -> OrderStatus {
        match status {
            "New" | "PartiallyFilled" => OrderStatus::Created,
            "Filled" => OrderStatus::Completed,
            "Canceled" | "Expired" | "Stopped" => OrderStatus::Canceled,
            "Rejected" => OrderStatus::FailedToCreate,
            _ => panic!("Bitmex: unexpected order status {}", status),
        }
    }

    #[named]
    pub(super) async fn request_order_info(
        &self,
        order: &OrderRef,
    ) -> Result<RestResponse, ExchangeError> {
        let client_order_id = order.client_order_id();

        let mut builder = UriBuilder::from_path("/api/v1/order");
        builder.add_kv(
            "filter",
            format_args!(
                "{}{client_order_id}{}",
                encode!(r#"{"clOrdID": "#),
                encode!("}"),
            ),
        );

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);
        let log_args = format!("order {client_order_id}");

        self.rest_client.get(uri, function_name!(), log_args).await
    }

    pub(super) fn parse_order_info(&self, response: &RestResponse) -> Result<OrderInfo> {
        let specific_orders: Vec<BitmexOrderInfo> = serde_json::from_str(&response.content)
            .context("Unable to parse response content for get_order_info request")?;

        let order = specific_orders
            .first()
            .context("No one order info received")?;

        Ok(self.specific_order_info_to_unified(order))
    }

    #[named]
    pub(super) async fn do_cancel_order(
        &self,
        order: OrderCancelling,
    ) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/api/v1/order");
        // Order may be canceled passing either exchange_order_id ("orderID" key) or client_order_id ("clOrdID" key)
        builder.add_kv("orderID", &order.exchange_order_id);

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);
        let log_args = format!("Cancel order for {}", order.header.client_order_id);

        self.rest_client
            .delete(uri, function_name!(), log_args)
            .await
    }

    #[named]
    pub(super) async fn do_cancel_all_orders(&self) -> Result<RestResponse, ExchangeError> {
        let builder = UriBuilder::from_path("/api/v1/order/all");

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);
        let log_args = "Cancel all orders".to_owned();

        self.rest_client
            .delete(uri, function_name!(), log_args)
            .await
    }

    pub(super) fn create_signature(secret_key: &str, message: &str, expire_time: u64) -> [u8; 64] {
        let mut hmac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
            .expect("Unable to calculate hmac for Bitmex signature");
        hmac.update(message.as_bytes());

        let mut expire_time_array = ArrayVec::<u8, 20>::new();
        write!(expire_time_array, "{expire_time}").expect("Failed to convert UNIX time to string");
        hmac.update(expire_time_array.as_slice());

        let hmac_bytes = hmac.finalize().into_bytes();

        let mut hex_array = [0u8; 64];
        write!(hex_array.as_slice_mut(), "{:x}", hmac_bytes)
            .expect("Failed to convert signature bytes array to hex");

        hex_array
    }

    pub(super) fn get_key_expire_time(secs: u64) -> u64 {
        let current_unix_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System Time before UNIX EPOCH!")
            .as_secs();

        current_unix_time + secs
    }

    #[named]
    pub(super) async fn request_my_trades(
        &self,
        symbol: &Symbol,
        last_date_time: Option<DateTime>,
    ) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/api/v1/execution/tradeHistory");
        builder.add_kv(
            "symbol",
            self.get_specific_currency_pair(symbol.currency_pair()),
        );
        builder.add_kv("reverse", true);
        builder.add_kv("count", 100);
        if let Some(date_time) = last_date_time {
            builder.add_kv("startTime", date_time.to_rfc3339());
        }

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_my_trades(&self, response: &RestResponse) -> Result<Vec<OrderTrade>> {
        let trade_data: Vec<BitmexOrderFill> =
            serde_json::from_str(&response.content).context("Failed to parse trade data")?;

        trade_data
            .into_iter()
            .filter_map(|variant| match variant {
                BitmexOrderFill::Trade(trade) => Some(Ok(OrderTrade {
                    exchange_order_id: trade.exchange_order_id,
                    trade_id: trade.trade_id,
                    datetime: trade.timestamp,
                    price: trade.fill_price,
                    amount: trade.fill_amount,
                    order_role: Bitmex::get_order_role_by_commission_amount(
                        trade.commission_amount,
                    ),
                    fee_currency_code: trade.currency.into(),
                    fee_rate: Some(trade.commission_rate),
                    fee_amount: Some(trade.commission_amount),
                    fill_type: Self::get_order_fill_type(&trade.details).ok()?,
                })),
                BitmexOrderFill::Funding(_) => None,
            })
            .try_collect()
    }

    #[named]
    pub(super) async fn request_get_position(&self) -> Result<RestResponse, ExchangeError> {
        let builder = UriBuilder::from_path("/api/v1/position");
        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_get_position(
        &self,
        response: &RestResponse,
    ) -> Result<Vec<ActivePosition>> {
        Ok(self
            .get_derivative_positions(response)?
            .into_iter()
            .map(ActivePosition::new)
            .collect_vec())
    }

    #[named]
    pub(super) async fn request_close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/api/v1/order");
        builder.add_kv(
            "symbol",
            self.get_specific_currency_pair(position.derivative.currency_pair),
        );
        builder.add_kv("execInst", "Close");
        builder.add_kv("text", encode!("Position Close via API."));
        if let Some(price_value) = price {
            builder.add_kv("price", price_value);
        }

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        let log_args = format!("Close position response for {position:?} {price:?}");
        self.rest_client
            .post(uri, None, function_name!(), log_args)
            .await
    }

    pub(super) fn parse_close_position(&self, response: &RestResponse) -> Result<ClosedPosition> {
        let bitmex_order: BitmexOrderInfo = serde_json::from_str(&response.content)
            .context("Unable to parse response content for close_position() request")?;
        let amount = bitmex_order
            .amount
            .context("Missing amount value in Bitmex order info")?;

        Ok(ClosedPosition::new(bitmex_order.exchange_order_id, amount))
    }

    #[named]
    pub(super) async fn request_get_balance(&self) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/api/v1/user/margin");
        builder.add_kv("currency", "all");

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_get_balance(
        &self,
        response: &RestResponse,
    ) -> Result<ExchangeBalancesAndPositions> {
        let raw_balances: Vec<BitmexBalanceInfo> =
            serde_json::from_str(&response.content).context("Failed to parse balance")?;

        let currency_rates = self.currency_balance_rates.lock();
        let common_balances = raw_balances
            .into_iter()
            .map(|balance_info| {
                let currency_code = CurrencyCode::from(balance_info.currency);
                let balance_rate = currency_rates.get(&currency_code).ok_or_else(|| {
                    anyhow!("Balance rate not found for currency {currency_code}")
                })?;

                Result::<_, anyhow::Error>::Ok(ExchangeBalance {
                    currency_code,
                    balance: balance_info.balance * balance_rate,
                })
            })
            .try_collect()?;

        Ok(ExchangeBalancesAndPositions {
            balances: common_balances,
            positions: None,
        })
    }

    pub(super) fn parse_balance_and_positions(
        &self,
        balance_response: &RestResponse,
        positions_response: &RestResponse,
    ) -> Result<ExchangeBalancesAndPositions> {
        let derivative = self.get_derivative_positions(positions_response)?;
        let mut balance = self.parse_get_balance(balance_response)?;
        balance.positions = Some(derivative);

        Ok(balance)
    }

    fn get_derivative_positions(&self, response: &RestResponse) -> Result<Vec<DerivativePosition>> {
        let bitmex_positions: Vec<PositionPayload> =
            serde_json::from_str(&response.content).context("Failed to parse positions")?;

        bitmex_positions
            .into_iter()
            .map(|position| {
                let currency_pair = self.get_unified_currency_pair(&position.symbol)?;
                Ok(DerivativePosition {
                    currency_pair,
                    position: position.amount,
                    average_entry_price: position.average_entry_price.unwrap_or_default(),
                    liquidation_price: position.liquidation_price.unwrap_or_default(),
                    leverage: position.leverage,
                })
            })
            .try_collect()
    }

    pub(super) fn get_order_role_by_commission_amount(commission_amount: Decimal) -> OrderRole {
        if commission_amount.is_sign_positive() {
            OrderRole::Taker
        } else {
            OrderRole::Maker
        }
    }

    pub(super) async fn update_currency_assets(&self) -> Result<()> {
        let response = self.request_wallet_assets().await?;

        self.parse_wallet_assets(&response)
    }

    #[named]
    async fn request_wallet_assets(&self) -> Result<RestResponse, ExchangeError> {
        let builder = UriBuilder::from_path("/api/v1/wallet/assets");
        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    fn parse_wallet_assets(&self, response: &RestResponse) -> Result<()> {
        let assets: Vec<BitmexWalletAsset> = serde_json::from_str(&response.content)
            .context("Failed to parse wallet assets response")?;
        let mut currency_rates = self.currency_balance_rates.lock();

        for asset in assets {
            currency_rates.insert(asset.currency.into(), dec!(0.1).powi(asset.scale as i64));
        }

        Ok(())
    }
}

pub struct BitmexBuilder;

impl ExchangeClientBuilder for BitmexBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
        _timeout_manager: Arc<TimeoutManager>,
        _orders: Arc<OrdersPool>,
    ) -> ExchangeClientBuilderResult {
        ExchangeClientBuilderResult {
            client: Box::new(Bitmex::new(
                exchange_settings,
                events_channel,
                lifetime_manager,
            )),
            features: ExchangeFeatures {
                open_orders_type: OpenOrdersType::AllCurrencyPair,
                rest_fills_features: RestFillsFeatures::new(RestFillsType::MyTrades),
                order_features: OrderFeatures {
                    maker_only: true,
                    supports_get_order_info_by_client_order_id: true,
                    cancellation_response_from_rest_only_for_errors: true,
                    creation_response_from_rest_only_for_errors: true,
                    order_was_completed_error_for_cancellation: true,
                    supports_already_cancelled_order: true,
                    supports_stop_loss_order: true,
                },
                trade_option: OrderTradeOption {
                    supports_trade_time: true,
                    supports_trade_incremented_id: false,
                    notification_on_each_currency_pair: false,
                    supports_get_prints: true,
                    supports_tick_direction: true,
                    supports_my_trades_from_time: true,
                },
                websocket_options: WebSocketOptions {
                    execution_notification: true,
                    cancellation_notification: true,
                    supports_ping_pong: true,
                    supports_subscription_response: false,
                },
                empty_response_is_ok: EMPTY_RESPONSE_IS_OK,
                balance_position_option: BalancePositionOption::NonDerivative,
                allowed_create_event_source_type: AllowedEventSourceType::All,
                allowed_fill_event_source_type: AllowedEventSourceType::All,
                allowed_cancel_event_source_type: AllowedEventSourceType::All,
            },
        }
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        RequestTimeoutArguments::from_requests_per_minute(60)
    }

    fn get_exchange_id(&self) -> ExchangeId {
        "Bitmex".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::ByteSlice;

    #[test]
    fn generate_signature() {
        // Test data from https://www.bitmex.com/app/apiKeysUsage
        let api_key = "LAqUlngMIQkIUjXMUreyu3qn".to_owned();
        let secret_key = "chNOOS4KvNXR_Xq4k4c9qsfoKWvnDecLATCRlcBwyKDYnWgO".to_owned();
        let path = "/api/v1/instrument?filter=%7B%22symbol%22%3A+%22XBTM15%22%7D";
        let expire_time = 1518064237;

        let rest_header = RestHeadersBitmex {
            api_key,
            secret_key,
        };

        let (message, _) = RestHeadersBitmex::create_signature_message(path, RequestType::Get);

        let signature_hash =
            Bitmex::create_signature(&rest_header.secret_key, message.as_str(), expire_time);

        assert_eq!(
            signature_hash
                .to_str()
                .expect("Failed to convert signature hash to string"),
            "e2f422547eecb5b3cb29ade2127e21b858b235b386bfa45e1c1756eb3383919f"
        );
    }
}
