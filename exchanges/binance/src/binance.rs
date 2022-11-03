use anyhow::{anyhow, bail, Context, Result};
use dashmap::DashMap;
use function_name::named;
use hmac::digest::generic_array;
use hmac::{Hmac, Mac};
use hyper::header::CONTENT_TYPE;
use hyper::http::request::Builder;
use hyper::Uri;
use itertools::Itertools;
use mmb_utils::time::{get_current_milliseconds, u64_to_date_time};
use mmb_utils::DateTime;
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::broadcast;

use super::support::{BinanceOrderInfo, BinanceSpotBalances};
use crate::support::{BinanceAccountInfo, BinanceMarginBalances, BinancePosition};
use mmb_core::exchanges::general::exchange::BoxExchangeClient;
use mmb_core::exchanges::general::exchange::Exchange;
use mmb_core::exchanges::general::features::{
    OrderFeatures, OrderTradeOption, RestFillsFeatures, RestFillsType, WebSocketOptions,
};
use mmb_core::exchanges::general::handlers::handle_order_filled::FillAmount;
use mmb_core::exchanges::general::handlers::handle_order_filled::FillEvent;
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::rest_client::{
    ErrorHandler, ErrorHandlerData, RequestType, RestClient, RestHeaders, RestResponse, UriBuilder,
};
use mmb_core::exchanges::timeouts::timeout_manager::TimeoutManager;
use mmb_core::exchanges::traits::{ExchangeClientBuilder, ExchangeError};
use mmb_core::exchanges::traits::{
    ExchangeClientBuilderResult, HandleOrderFilledCb, HandleTradeCb, OrderCancelledCb,
    OrderCreatedCb, Support,
};
use mmb_core::exchanges::{
    general::features::{ExchangeFeatures, OpenOrdersType},
    timeouts::requests_timeout_manager_factory::RequestTimeoutArguments,
};
use mmb_core::lifecycle::app_lifetime_manager::AppLifetimeManager;
use mmb_core::settings::ExchangeSettings;
use mmb_domain::events::AllowedEventSourceType;
use mmb_domain::events::{ExchangeBalance, ExchangeBalancesAndPositions, ExchangeEvent, TradeId};
use mmb_domain::exchanges::symbol::{Precision, Symbol};
use mmb_domain::market::{CurrencyCode, CurrencyId, CurrencyPair, ExchangeErrorType, ExchangeId};
use mmb_domain::market::{ExchangeAccountId, SpecificCurrencyPair};
use mmb_domain::order::fill::{EventSourceType, OrderFillType};
use mmb_domain::order::pool::{OrderRef, OrdersPool};
use mmb_domain::order::snapshot::*;
use mmb_domain::order::snapshot::{Amount, Price};
use mmb_domain::position::{ActivePosition, DerivativePosition};
use mmb_utils::value_to_decimal::GetOrErr;
use serde::{Deserialize, Serialize};
use sha2::digest::generic_array::GenericArray;

const LISTEN_KEY: &str = "listenKey";

#[derive(Default)]
pub struct ErrorHandlerBinance;

pub struct RestHeadersBinance {
    pub api_key: String,
    pub is_usd_m_futures: bool,
}

impl RestHeaders for RestHeadersBinance {
    fn add_specific_headers(
        &self,
        builder: Builder,
        _uri: &Uri,
        _request_type: RequestType,
    ) -> Builder {
        match self.is_usd_m_futures {
            true => builder.header(CONTENT_TYPE, "application/x-www-form-urlencoded"),
            false => builder,
        }
        .header("X-MBX-APIKEY", &self.api_key)
    }
}

impl ErrorHandler for ErrorHandlerBinance {
    fn check_spec_rest_error(&self, response: &RestResponse) -> Result<(), ExchangeError> {
        //Binance is a little inconsistent: for failed responses sometimes they include
        //only code or only success:false but sometimes both
        if !(response.content.contains(r#""success":false"#)
            || response.content.contains(r#""code""#))
        {
            return Ok(());
        }

        #[derive(Deserialize)]
        struct Error {
            msg: String,
            code: i64,
        }

        let error: Error = serde_json::from_str(&response.content).map_err(|err| {
            ExchangeError::parsing(format!(
                "Unable to parse response.content: {err:?}\n{}",
                response.content
            ))
        })?;

        Err(ExchangeError::new(
            ExchangeErrorType::Unknown,
            error.msg,
            Some(error.code),
        ))
    }

    fn clarify_error_type(&self, error: &ExchangeError) -> ExchangeErrorType {
        use ExchangeErrorType::*;
        // -1010 ERROR_MSG_RECEIVED
        // -2010 NEW_ORDER_REJECTED
        // -2011 CANCEL_REJECTED
        match error.message.as_str() {
            "Unknown order sent." | "Order does not exist." => OrderNotFound,
            "Account has insufficient balance for requested action." => InsufficientFunds,
            "Invalid quantity."
            | "Filter failure: MIN_NOTIONAL"
            | "Filter failure: LOT_SIZE"
            | "Filter failure: PRICE_FILTER"
            | "Filter failure: PERCENT_PRICE"
            | "Quantity less than zero."
            | "Precision is over the maximum defined for this asset." => InvalidOrder,
            msg if msg.contains("Too many requests;") => RateLimit,
            _ => Unknown,
        }
    }
}

const EMPTY_RESPONSE_IS_OK: bool = false;

pub struct Binance {
    pub settings: ExchangeSettings,
    pub hosts: Hosts,
    pub id: ExchangeAccountId,
    pub order_created_callback: OrderCreatedCb,
    pub order_cancelled_callback: OrderCancelledCb,
    pub handle_order_filled_callback: HandleOrderFilledCb,
    pub handle_trade_callback: HandleTradeCb,

    pub unified_to_specific: RwLock<HashMap<CurrencyPair, SpecificCurrencyPair>>,
    pub specific_to_unified: RwLock<HashMap<SpecificCurrencyPair, CurrencyPair>>,
    pub supported_currencies: DashMap<CurrencyId, CurrencyCode>,

    // currencies specified in settings for exchange
    pub working_currencies_ids: RwLock<Vec<CurrencyId>>,
    pub(super) timeout_manager: Arc<TimeoutManager>,

    // Currencies used for trading according to user settings
    pub(super) traded_specific_currencies: Mutex<Vec<SpecificCurrencyPair>>,

    pub(super) last_trade_ids: DashMap<CurrencyPair, TradeId>,

    pub(super) lifetime_manager: Arc<AppLifetimeManager>,

    pub(super) events_channel: broadcast::Sender<ExchangeEvent>,

    pub(super) subscribe_to_market_data: bool,
    pub(super) is_reducing_market_data: bool,

    pub(super) rest_client: RestClient<ErrorHandlerBinance, RestHeadersBinance>,

    // NOTE: None when websocket is disconnected
    pub(super) listen_key: RwLock<Option<String>>,
}

impl Binance {
    pub(crate) fn initialize_working_currencies(&self, exchange: &Arc<Exchange>) {
        let currency_codes: HashSet<CurrencyCode> = exchange
            .symbols
            .iter()
            .flat_map(|x| x.key().to_codes().to_array())
            .collect();

        let currency_ids = self
            .supported_currencies
            .iter()
            .filter_map(|x| {
                if currency_codes.contains(x.value()) {
                    Some(*x.key())
                } else {
                    None
                }
            })
            .collect_vec();
        *self.working_currencies_ids.write() = currency_ids;
    }
}

impl Binance {
    pub fn new(
        id: ExchangeAccountId,
        settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
        timeout_manager: Arc<TimeoutManager>,
        is_reducing_market_data: bool,
    ) -> Self {
        let is_reducing_market_data = settings
            .is_reducing_market_data
            .unwrap_or(is_reducing_market_data);

        let hosts = Self::make_hosts(settings.is_margin_trading);
        let exchange_account_id = settings.exchange_account_id;

        Self {
            id,
            order_created_callback: Box::new(|_, _, _| {}),
            order_cancelled_callback: Box::new(|_, _, _| {}),
            handle_order_filled_callback: Box::new(|_| {}),
            handle_trade_callback: Box::new(|_, _| {}),
            unified_to_specific: Default::default(),
            specific_to_unified: Default::default(),
            supported_currencies: Default::default(),
            working_currencies_ids: Default::default(),
            traded_specific_currencies: Default::default(),
            last_trade_ids: Default::default(),
            subscribe_to_market_data: settings.subscribe_to_market_data,
            rest_client: RestClient::new(
                ErrorHandlerData::new(
                    EMPTY_RESPONSE_IS_OK,
                    exchange_account_id,
                    ErrorHandlerBinance::default(),
                ),
                RestHeadersBinance {
                    api_key: settings.api_key.clone(),
                    is_usd_m_futures: settings.is_margin_trading,
                },
            ),
            timeout_manager,
            is_reducing_market_data,
            settings,
            hosts,
            events_channel,
            lifetime_manager,
            listen_key: Default::default(),
        }
    }

    pub fn make_hosts(is_margin_trading: bool) -> Hosts {
        if is_margin_trading {
            Hosts {
                web_socket_host: "wss://fstream.binance.com",
                web_socket2_host: "wss://fstream.binance.com",
                rest_host: "https://fapi.binance.com",
            }
        } else {
            Hosts {
                web_socket_host: "wss://stream.binance.com:9443",
                web_socket2_host: "wss://stream.binance.com:9443",
                rest_host: "https://api.binance.com",
            }
        }
    }

    #[named]
    pub(super) async fn request_listen_key(&self) -> Result<RestResponse, ExchangeError> {
        let path = self.get_uri_path("/fapi/v1/listenKey", "/api/v3/userDataStream");
        let builder = UriBuilder::from_path(path);
        let (uri, query) = builder.build_uri_and_query(self.hosts.rest_uri_host(), false);

        self.rest_client
            .post(uri, Some(query), function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_listen_key(request_outcome: &RestResponse) -> Result<String> {
        let data: Value = serde_json::from_str(&request_outcome.content)
            .context("Unable to parse listen key response for Binance")?;

        let listen_key = data[LISTEN_KEY]
            .as_str()
            .context("Unable to parse listen key field for Binance")?
            .to_string();

        Ok(listen_key)
    }

    #[named]
    pub async fn request_update_listen_key(&self, listen_key: &str) -> Result<(), ExchangeError> {
        let path = self.get_uri_path("/fapi/v1/listenKey", "/api/v3/userDataStream");
        let mut builder = UriBuilder::from_path(path);
        builder.add_kv(LISTEN_KEY, listen_key);
        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .put(uri, function_name!(), "".to_string())
            .await
            .map(|_| ())
    }

    // TODO Change to pub(super) or pub(crate) after implementation if possible
    pub async fn reconnect(&mut self) {
        todo!("reconnect")
    }

    pub(super) fn get_stream_name(
        specific_currency_pair: &SpecificCurrencyPair,
        channel: &str,
    ) -> String {
        format!("{specific_currency_pair}@{channel}")
    }

    fn _is_websocket_reconnecting(&self) -> bool {
        todo!("is_websocket_reconnecting")
    }

    fn write_signature_to_builder(&self, builder: &mut UriBuilder) {
        let mut hmac = Hmac::<Sha256>::new_from_slice(self.settings.secret_key.as_bytes())
            .expect("Unable to calculate hmac for Binance signature");
        hmac.update(builder.query());

        let hmac_bytes = hmac.finalize().into_bytes();

        // hex representation of signature have double size of input data
        builder.ensure_free_size(hmac_bytes.len() * 2);

        struct HexAdapter<'a> {
            bytes: &'a GenericArray<u8, generic_array::typenum::U32>,
        }
        impl<'a> Display for HexAdapter<'a> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:x}", self.bytes)
            }
        }

        let hexer = HexAdapter { bytes: &hmac_bytes };
        builder.add_kv("signature", hexer);
    }

    pub(super) fn add_authentification(&self, builder: &mut UriBuilder) {
        let time_stamp = get_current_milliseconds();
        builder.add_kv("timestamp", time_stamp);

        self.write_signature_to_builder(builder);
    }

    pub(super) fn get_unified_currency_pair(
        &self,
        currency_pair: &SpecificCurrencyPair,
    ) -> Result<CurrencyPair> {
        self.specific_to_unified
            .read()
            .get(currency_pair)
            .cloned()
            .with_context(|| format!("Not found currency pair '{currency_pair:?}' in {}", self.id))
    }

    pub(super) fn specific_order_info_to_unified(&self, specific: &BinanceOrderInfo) -> OrderInfo {
        OrderInfo::new(
            self.get_unified_currency_pair(&specific.specific_currency_pair)
                .expect("expected known currency pair"),
            specific.exchange_order_id.to_string().as_str().into(),
            specific.client_order_id.clone(),
            get_local_order_side(&specific.side),
            get_local_order_status(&specific.status),
            specific.price,
            specific.orig_quantity,
            specific.price,
            specific.executed_quantity,
            None,
            None,
            None,
        )
    }

    pub(super) fn handle_order_fill(
        &self,
        msg_to_log: &str,
        json_response: Value,
        event_time: DateTime,
    ) -> Result<()> {
        // TODO need special handler for OCO orders
        let client_order_id = json_response["c"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse client order id"))?;

        let exchange_order_id = json_response["i"].to_string();
        let exchange_order_id = exchange_order_id.trim_matches('"');
        let execution_type = json_response["x"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse execution type"))?;
        let order_status = json_response["X"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse order status"))?;
        let time_in_force = json_response["f"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse time in force"))?;

        match execution_type {
            "NEW" => match order_status {
                "NEW" => {
                    (self.order_created_callback)(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => log::error!("execution_type is NEW but order_status is {order_status} for message {msg_to_log}"),
            },
            "CANCELED" => match order_status {
                "CANCELED" => {
                    (self.order_cancelled_callback)(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => log::error!("execution_type is CANCELED but order_status is {order_status} for message {msg_to_log}"),
            },
            "REJECTED" => {
                // TODO: May be not handle error in Rest but move it here to make it unified?
                // We get notification of rejected orders from the rest responses
            }
            "EXPIRED" => match time_in_force {
                "GTX" => {
                    (self.order_cancelled_callback)(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => log::error!("Order {client_order_id} was expired, message: {msg_to_log}"),
            },
            "TRADE" | "CALCULATED" => {
                let event_data = self.prepare_data_for_fill_handler(
                    &json_response,
                    execution_type,
                    client_order_id.into(),
                    exchange_order_id.into(),
                    event_time,
                )?;

                (self.handle_order_filled_callback)(event_data);
            }
            _ => log::error!("Impossible execution type"),
        }

        Ok(())
    }

    pub(crate) fn get_currency_code(&self, currency_id: &CurrencyId) -> Option<CurrencyCode> {
        self.supported_currencies
            .get(currency_id)
            .map(|some| *some.value())
    }

    fn prepare_data_for_fill_handler(
        &self,
        json_response: &Value,
        execution_type: &str,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
        event_time: DateTime,
    ) -> Result<FillEvent> {
        let trade_id = json_response["t"].clone().into();
        let last_filled_price = json_response["L"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse last filled price"))?;
        let last_filled_amount = json_response["l"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse last filled amount"))?;
        let total_filled_amount = json_response["z"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse total filled amount"))?;
        let commission_amount = json_response["n"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse last commission amount"))?;
        let commission_currency = json_response["N"]
            .as_str()
            .ok_or_else(|| anyhow!("Unable to parse last commission currency"))?;
        let commission_currency_code = self
            .get_currency_code(&commission_currency.into())
            .ok_or_else(|| anyhow!("There are no such supported currency code"))?;
        let is_maker = json_response["m"]
            .as_bool()
            .ok_or_else(|| anyhow!("Unable to parse trade side"))?;

        let fill_type = Self::get_fill_type(execution_type)?;
        let order_role = if is_maker {
            OrderRole::Maker
        } else {
            OrderRole::Taker
        };

        let fill_amount = FillAmount::Incremental {
            fill_amount: last_filled_amount.parse()?,
            total_filled_amount: Some(total_filled_amount.parse()?),
        };

        let fill_event = FillEvent {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id),
            client_order_id: Some(client_order_id),
            exchange_order_id,
            fill_price: last_filled_price.parse()?,
            fill_amount,
            order_role: Some(order_role),
            commission_currency_code: Some(commission_currency_code),
            commission_rate: None,
            commission_amount: Some(commission_amount.parse()?),
            fill_type,
            special_order_data: None,
            fill_date: Some(event_time),
        };

        Ok(fill_event)
    }

    // According to https://binance-docs.github.io/apidocs/futures/en/#event-order-update
    fn get_fill_type(raw_type: &str) -> Result<OrderFillType> {
        match raw_type {
            "CALCULATED" => Ok(OrderFillType::Liquidation),
            "FILL" | "TRADE" | "PARTIAL_FILL" => Ok(OrderFillType::UserTrade),
            _ => bail!("Unable to map trade type"),
        }
    }

    pub(super) fn get_spot_exchange_balances_and_positions(
        &self,
        raw_balances: Vec<BinanceSpotBalances>,
    ) -> ExchangeBalancesAndPositions {
        let balances = raw_balances
            .iter()
            .filter_map(|balance| {
                self.get_currency_code(&balance.asset.as_str().into())
                    .map(|currency_code| ExchangeBalance {
                        currency_code,
                        balance: balance.free,
                    })
            })
            .collect_vec();

        ExchangeBalancesAndPositions {
            balances,
            positions: None,
        }
    }

    pub(super) fn get_margin_exchange_balances_and_positions(
        &self,
        raw_balances: Vec<BinanceMarginBalances>,
    ) -> ExchangeBalancesAndPositions {
        let balances = raw_balances
            .iter()
            .filter_map(|balance| {
                self.get_currency_code(&balance.asset.as_str().into())
                    .map(|currency_code| ExchangeBalance {
                        currency_code,
                        balance: balance.available_balance,
                    })
            })
            .collect_vec();

        ExchangeBalancesAndPositions {
            balances,
            positions: None,
        }
    }

    pub(super) fn get_order_id(
        &self,
        response: &RestResponse,
    ) -> Result<ExchangeOrderId, ExchangeError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct OrderId {
            order_id: u64,
        }

        let deserialized: OrderId = serde_json::from_str(&response.content)
            .map_err(|err| ExchangeError::parsing(format!("Unable to parse orderId: {err:?}")))?;

        let order_id_str = deserialized.order_id.to_string().into();
        Ok(ExchangeOrderId::new(order_id_str))
    }

    pub(super) fn get_uri_path<'a>(
        &self,
        margin_trading_url: &'a str,
        not_margin_trading_url: &'a str,
    ) -> &'a str {
        match self.settings.is_margin_trading {
            true => margin_trading_url,
            false => not_margin_trading_url,
        }
    }

    #[named]
    pub(crate) async fn request_open_orders_by_http_header(
        &self,
        builder: UriBuilder,
    ) -> Result<RestResponse, ExchangeError> {
        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    #[named]
    pub(super) async fn request_order_info(
        &self,
        order: &OrderRef,
    ) -> Result<RestResponse, ExchangeError> {
        let (currency_pair, client_order_id) =
            order.fn_ref(|x| (x.currency_pair(), x.client_order_id()));

        let specific_currency_pair = self.get_specific_currency_pair(currency_pair);

        let path = self.get_uri_path("/fapi/v1/order", "/api/v3/order");
        let mut builder = UriBuilder::from_path(path);
        builder.add_kv("symbol", &specific_currency_pair);
        builder.add_kv("origClientOrderId", &client_order_id);
        self.add_authentification(&mut builder);
        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        let log_args = format!("order {client_order_id}");

        self.rest_client.get(uri, function_name!(), log_args).await
    }

    pub(super) fn parse_order_info(&self, response: &RestResponse) -> OrderInfo {
        let specific_order: BinanceOrderInfo = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_order_info request");

        self.specific_order_info_to_unified(&specific_order)
    }

    fn get_open_order_path(&self) -> &str {
        self.get_uri_path("/fapi/v1/openOrders", "/api/v3/openOrders")
    }

    pub(super) async fn request_open_orders(&self) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path(self.get_open_order_path());
        self.add_authentification(&mut builder);

        self.request_open_orders_by_http_header(builder).await
    }

    pub(super) async fn request_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<RestResponse, ExchangeError> {
        let specific_currency_pair = self.get_specific_currency_pair(currency_pair);

        let mut builder = UriBuilder::from_path(self.get_open_order_path());
        builder.add_kv("symbol", &specific_currency_pair);
        self.add_authentification(&mut builder);

        self.request_open_orders_by_http_header(builder).await
    }

    pub(super) fn parse_open_orders(&self, response: &RestResponse) -> Vec<OrderInfo> {
        let binance_orders: Vec<BinanceOrderInfo> = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_open_orders request");

        binance_orders
            .iter()
            .map(|order| self.specific_order_info_to_unified(order))
            .collect()
    }

    #[named]
    pub(super) async fn request_close_position(
        &self,
        position: &ActivePosition,
        price: Option<Price>,
    ) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/fapi/v1/order");
        builder.add_kv("quantity", position.derivative.position.abs());
        let side = position.derivative.get_side().change_side();
        builder.add_kv("side", get_server_order_side(side));
        builder.add_kv(
            "symbol",
            self.get_specific_currency_pair(position.derivative.currency_pair),
        );

        match price {
            Some(price_value) => {
                builder.add_kv("type", "LIMIT");
                builder.add_kv("price", price_value);
                builder.add_kv("timeInForce", "GTC");
            }
            None => builder.add_kv("type", "MARKET"),
        }

        self.add_authentification(&mut builder);

        let (uri, query) = builder.build_uri_and_query(self.hosts.rest_uri_host(), false);

        let log_args = format!("Close position response for {position:?} {price:?}");
        self.rest_client
            .post(uri, Some(query), function_name!(), log_args)
            .await
    }

    #[named]
    pub(super) async fn request_get_position(&self) -> Result<RestResponse, ExchangeError> {
        let mut builder = UriBuilder::from_path("/fapi/v2/positionRisk");
        self.add_authentification(&mut builder);

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_active_positions(
        &self,
        response: &RestResponse,
    ) -> Result<Vec<ActivePosition>> {
        self.get_derivative_positions(response)?
            .map(|position| Ok(ActivePosition::new(position?)))
            .try_collect()
    }

    fn get_derivative_positions<'a>(
        &'a self,
        response: &RestResponse,
    ) -> Result<impl Iterator<Item = Result<DerivativePosition>> + 'a> {
        let binance_positions: Vec<BinancePosition> =
            serde_json::from_str(&response.content).context("Unable to parse Binance positions")?;

        let unified_currency_pairs = self.specific_to_unified.read();
        Ok(binance_positions
            .into_iter()
            // Binance returns all possible positions not only active
            .filter(|position| !position.position_amount.is_zero())
            .map(move |position| {
                let currency_pair = unified_currency_pairs
                    .get(&position.specific_currency_pair)
                    .with_context(|| {
                        format!(
                            "Failed to get_unified_currency_pair for {:?}",
                            position.specific_currency_pair
                        )
                    })?;

                Ok(DerivativePosition::new(
                    *currency_pair,
                    position.position_amount,
                    position.average_entry_price,
                    position.liquidation_price,
                    position.leverage,
                ))
            }))
    }

    #[named]
    pub(super) async fn request_get_balance(&self) -> Result<RestResponse, ExchangeError> {
        let path = self.get_uri_path("/fapi/v2/account", "/api/v3/account");
        let mut builder = UriBuilder::from_path(path);
        self.add_authentification(&mut builder);
        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_get_balance(
        &self,
        response: &RestResponse,
    ) -> Result<ExchangeBalancesAndPositions> {
        let binance_account_info: BinanceAccountInfo =
            serde_json::from_str(&response.content).context("Unable to parse account info")?;

        Ok(match self.settings.is_margin_trading {
            true => self.get_margin_exchange_balances_and_positions(
                binance_account_info
                    .assets
                    .context("Unable to parse margin balances")?,
            ),
            false => self.get_spot_exchange_balances_and_positions(
                binance_account_info
                    .balances
                    .context("Unable to parse spot balances")?,
            ),
        })
    }

    pub(super) fn parse_balance_and_positions(
        &self,
        balance_response: &RestResponse,
        positions_response: &RestResponse,
    ) -> Result<ExchangeBalancesAndPositions> {
        let derivative = self
            .get_derivative_positions(positions_response)?
            .try_collect()?;
        let mut balances_and_positions = self.parse_get_balance(balance_response)?;
        balances_and_positions.positions = Some(derivative);

        Ok(balances_and_positions)
    }

    #[named]
    pub(super) async fn request_cancel_order(
        &self,
        order: OrderCancelling,
    ) -> Result<RestResponse, ExchangeError> {
        let specific_currency_pair = self.get_specific_currency_pair(order.header.currency_pair);

        let path = self.get_uri_path("/fapi/v1/order", "/api/v3/order");
        let mut builder = UriBuilder::from_path(path);
        builder.add_kv("symbol", &specific_currency_pair);
        builder.add_kv("orderId", &order.exchange_order_id);
        self.add_authentification(&mut builder);

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        let log_args = format!("Cancel order for {}", order.header.client_order_id);
        self.rest_client
            .delete(uri, function_name!(), log_args)
            .await
    }

    #[named]
    pub(super) async fn request_my_trades(
        &self,
        symbol: &Symbol,
        last_date_time: Option<DateTime>,
    ) -> Result<RestResponse, ExchangeError> {
        let specific_currency_pair = self.get_specific_currency_pair(symbol.currency_pair());

        let path = self.get_uri_path("/fapi/v1/userTrades", "/api/v3/myTrades");
        let mut builder = UriBuilder::from_path(path);
        if let Some(last_date_time_value) = last_date_time {
            builder.add_kv(
                "startTime",
                last_date_time_value.timestamp_millis().to_string(),
            );
        }
        builder.add_kv("symbol", &specific_currency_pair);
        self.add_authentification(&mut builder);

        let uri = builder.build_uri(self.hosts.rest_uri_host(), true);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_get_my_trades(
        &self,
        response: &RestResponse,
        _last_date_time: Option<DateTime>,
    ) -> Result<Vec<OrderTrade>> {
        #[derive(Serialize, Deserialize, Debug)]
        #[serde(rename_all = "camelCase")]
        struct BinanceMyTrade {
            id: Value,
            order_id: u64,
            price: Price,
            #[serde(alias = "qty")]
            amount: Amount,
            commission: Amount,
            #[serde(alias = "commissionAsset")]
            commission_currency_code: CurrencyId,
            time: u64,
            #[serde(alias = "maker")]
            is_maker: bool,
        }

        impl BinanceMyTrade {
            fn to_unified_order_trade(
                &self,
                commission_currency_code: Option<CurrencyCode>,
            ) -> Result<OrderTrade> {
                let datetime: DateTime = (UNIX_EPOCH + Duration::from_millis(self.time)).into();
                let order_role = if self.is_maker {
                    OrderRole::Maker
                } else {
                    OrderRole::Taker
                };

                let fee_currency_code = commission_currency_code.context("There is no suitable currency code to get specific_currency_pair for unified_order_trade converting")?;
                Ok(OrderTrade::new(
                    self.order_id.into(),
                    TradeId::from(&self.id),
                    datetime,
                    self.price,
                    self.amount,
                    order_role,
                    fee_currency_code,
                    None,
                    Some(self.commission),
                    OrderFillType::UserTrade,
                ))
            }
        }

        let my_trades: Vec<BinanceMyTrade> =
            serde_json::from_str(&response.content).expect("Unable to parse trades from response");

        my_trades
            .into_iter()
            .map(|my_trade| {
                my_trade.to_unified_order_trade(
                    self.get_currency_code(&my_trade.commission_currency_code),
                )
            })
            .collect()
    }

    #[named]
    pub(super) async fn request_create_order(
        &self,
        order: &OrderRef,
    ) -> Result<RestResponse, ExchangeError> {
        let (header, price) = order.fn_ref(|order| (order.header.clone(), order.price()));
        let specific_currency_pair = self.get_specific_currency_pair(header.currency_pair);
        let is_margin_trading = self.settings.is_margin_trading;

        let path = self.get_uri_path("/fapi/v1/order", "/api/v3/order");
        let mut builder = UriBuilder::from_path(path);
        builder.add_kv("symbol", specific_currency_pair);
        builder.add_kv("side", get_server_order_side(header.side));
        builder.add_kv("type", get_server_order_type(&header, is_margin_trading));
        builder.add_kv("quantity", &header.amount);
        builder.add_kv("newClientOrderId", &header.client_order_id);

        if header.order_type != OrderType::Market {
            builder.add_kv("price", &price);
        }

        if header.order_type != OrderType::Market
            && header.execution_type != OrderExecutionType::MakerOnly
        {
            builder.add_kv("timeInForce", "GTC");
        } else if header.execution_type == OrderExecutionType::MakerOnly && is_margin_trading {
            builder.add_kv("timeInForce", "GTX");
        }

        self.add_authentification(&mut builder);

        let (uri, query) = builder.build_uri_and_query(self.hosts.rest_uri_host(), false);

        let log_args = format!("Create order for {header:?}");
        self.rest_client
            .post(uri, Some(query), function_name!(), log_args)
            .await
    }

    #[named]
    pub(super) async fn request_all_symbols(&self) -> Result<RestResponse, ExchangeError> {
        let path = self.get_uri_path("/fapi/v1/exchangeInfo", "/api/v3/exchangeInfo");
        let builder = UriBuilder::from_path(path);
        let uri = builder.build_uri(self.hosts.rest_uri_host(), false);

        self.rest_client
            .get(uri, function_name!(), "".to_string())
            .await
    }

    pub(super) fn parse_all_symbols(&self, response: &RestResponse) -> Result<Vec<Arc<Symbol>>> {
        let deserialized: Value = serde_json::from_str(&response.content)
            .expect("Unable to deserialize response from Binance");
        let symbols = deserialized
            .get("symbols")
            .and_then(|symbols| symbols.as_array())
            .ok_or_else(|| anyhow!("Unable to get symbols array from Binance"))?;

        let mut supported_symbols = Vec::new();
        for symbol in symbols {
            if Binance::is_unsupported_symbol(symbol) {
                continue;
            }

            let base_currency_id = &symbol
                .get_as_str("baseAsset")
                .expect("Unable to get base currency id from Binance");
            let quote_currency_id = &symbol
                .get_as_str("quoteAsset")
                .expect("Unable to get quote currency id from Binance");
            let base = base_currency_id.as_str().into();
            let quote = quote_currency_id.as_str().into();

            let specific_currency_pair_id = symbol
                .get_as_str("symbol")
                .expect("Unable to get specific currency pair");
            let specific_currency_pair = specific_currency_pair_id.as_str().into();
            let unified_currency_pair = CurrencyPair::from_codes(base, quote);
            self.unified_to_specific
                .write()
                .insert(unified_currency_pair, specific_currency_pair);

            self.specific_to_unified
                .write()
                .insert(specific_currency_pair, unified_currency_pair);

            let (amount_currency_code, balance_currency_code) =
                match self.settings.is_margin_trading {
                    // TODO need explicit specify type of using Binance futures markets
                    // true => (quote, Some(base)),
                    true => (base, Some(quote)),
                    false => (base, None),
                };

            let mut min_amount = None;
            let mut max_amount = None;
            let mut min_price = None;
            let mut max_price = None;
            let mut min_cost = None;
            let mut price_tick = None;
            let mut amount_tick = None;

            let filters = symbol
                .get("filters")
                .and_then(|filters| filters.as_array())
                .expect("Unable to get filters as array from Binance");
            for filter in filters {
                let filter_name = filter.get_as_str("filterType")?;
                match filter_name.as_str() {
                    "PRICE_FILTER" => {
                        min_price = filter.get_as_decimal("minPrice");
                        max_price = filter.get_as_decimal("maxPrice");
                        price_tick = filter.get_as_decimal("tickSize");
                    }
                    "LOT_SIZE" => {
                        min_amount = filter.get_as_decimal("minQty");
                        max_amount = filter.get_as_decimal("maxQty");
                        amount_tick = filter.get_as_decimal("stepSize");
                    }
                    "MIN_NOTIONAL" => {
                        min_cost = match self.settings.is_margin_trading {
                            true => filter.get_as_decimal("notional"),
                            false => filter.get_as_decimal("minNotional"),
                        };
                    }
                    _ => {}
                }
            }

            let price_precision = match price_tick {
                Some(tick) => Precision::ByTick { tick },
                None => bail!(
                    "Unable to get price precision from Binance for {:?}",
                    specific_currency_pair
                ),
            };

            let amount_precision = match amount_tick {
                Some(tick) => Precision::ByTick { tick },
                None => bail!(
                    "Unable to get amount precision from Binance for {:?}",
                    specific_currency_pair
                ),
            };

            let symbol = Symbol::new(
                self.settings.is_margin_trading,
                base_currency_id.as_str().into(),
                base,
                quote_currency_id.as_str().into(),
                quote,
                min_price,
                max_price,
                min_amount,
                max_amount,
                min_cost,
                amount_currency_code,
                balance_currency_code,
                price_precision,
                amount_precision,
            );

            supported_symbols.push(Arc::new(symbol))
        }

        Ok(supported_symbols)
    }

    fn is_unsupported_symbol(symbol: &Value) -> bool {
        let code = &symbol
            .get_as_str("symbol")
            .expect("Unable to get symbol code from Binance");

        // Binance adds "_<NUMBERS>" to old symbol's code
        code.contains('_') || symbol["status"] != "TRADING"
    }

    pub(super) fn get_event_time(data: &Value) -> Result<DateTime> {
        let event_time_u64 = data["E"]
            .as_u64()
            .ok_or_else(|| anyhow!("Unable to parse transaction time"))?;

        Ok(u64_to_date_time(event_time_u64))
    }
}

pub(super) fn get_server_order_side(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "BUY",
        OrderSide::Sell => "SELL",
    }
}

pub(super) fn get_local_order_side(side: &str) -> OrderSide {
    match side {
        "BUY" => OrderSide::Buy,
        "SELL" => OrderSide::Sell,
        _ => panic!("Unexpected order side"),
    }
}

fn get_local_order_status(status: &str) -> OrderStatus {
    match status {
        "NEW" | "PARTIALLY_FILLED" => OrderStatus::Created,
        "FILLED" => OrderStatus::Completed,
        "PENDING_CANCEL" => OrderStatus::Canceling,
        "CANCELED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        _ => panic!("Unexpected order status"),
    }
}

pub(super) fn get_server_order_type(header: &OrderHeader, is_margin_trading: bool) -> &'static str {
    if header.execution_type == OrderExecutionType::MakerOnly && !is_margin_trading {
        return "LIMIT_MAKER";
    }

    match header.order_type {
        OrderType::Limit => "LIMIT",
        OrderType::Market => "MARKET",
        unexpected_variant => panic!("{unexpected_variant:?} are not expected"),
    }
}

pub struct BinanceBuilder;

impl ExchangeClientBuilder for BinanceBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
        timeout_manager: Arc<TimeoutManager>,
        _orders: Arc<OrdersPool>,
    ) -> ExchangeClientBuilderResult {
        let exchange_account_id = exchange_settings.exchange_account_id;

        ExchangeClientBuilderResult {
            client: Box::new(Binance::new(
                exchange_account_id,
                exchange_settings,
                events_channel,
                lifetime_manager,
                timeout_manager,
                false,
            )) as BoxExchangeClient,
            features: ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::new(RestFillsType::None),
                OrderFeatures {
                    supports_get_order_info_by_client_order_id: true,
                    ..OrderFeatures::default()
                },
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                EMPTY_RESPONSE_IS_OK,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        }
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        RequestTimeoutArguments::from_requests_per_minute(1200)
    }

    fn get_exchange_id(&self) -> ExchangeId {
        "Binance".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmb_core::exchanges::timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory;
    use mmb_core::lifecycle::launcher::EngineBuildConfig;
    use mmb_utils::cancellation_token::CancellationToken;
    use mmb_utils::hashmap;

    pub(crate) fn get_timeout_manager(
        exchange_account_id: ExchangeAccountId,
    ) -> Arc<TimeoutManager> {
        let engine_build_config = EngineBuildConfig::new(vec![Box::new(BinanceBuilder)]);
        let timeout_arguments = engine_build_config.supported_exchange_clients
            [&exchange_account_id.exchange_id]
            .get_timeout_arguments();

        let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
            timeout_arguments,
            exchange_account_id,
        );

        TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
    }

    #[test]
    fn generate_signature() {
        // All values and strings gotten from binanсe API example
        let exchange_account_id: ExchangeAccountId = "Binance_0".parse().expect("in test");

        let settings = ExchangeSettings::new_short(
            exchange_account_id,
            "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A".into(),
            "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j".into(),
            false,
        );

        let (tx, _) = broadcast::channel(10);
        let binance = Binance::new(
            exchange_account_id,
            settings,
            tx,
            AppLifetimeManager::new(CancellationToken::default()),
            get_timeout_manager(exchange_account_id),
            false,
        );

        let mut builder = UriBuilder::from_path("/test");
        builder.add_kv("symbol", "LTCBTC");
        builder.add_kv("side", "BUY");
        builder.add_kv("type", "LIMIT");
        builder.add_kv("timeInForce", "GTC");
        builder.add_kv("quantity", "1");
        builder.add_kv("price", "0");
        builder.add_kv("recvWindow", "5000");
        builder.add_kv("timestamp", "1499827319559");
        binance.write_signature_to_builder(&mut builder);

        let query = builder.query();

        let expected = b"76f4fcd9c09d7969fcf97254950d690077f0fe090ea68ec7601a69ff36acd34b";

        //expected that signature was last parameter
        let signature_value = query.split_at(query.len() - expected.len()).1;

        assert_eq!(signature_value, expected);
    }
}
