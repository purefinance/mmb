use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use dashmap::DashMap;
use function_name::named;
use hex;
use hmac::{Hmac, Mac, NewMac};
use itertools::Itertools;
use mmb_utils::infrastructure::WithExpect;
use mmb_utils::time::{get_current_milliseconds, u64_to_date_time};
use mmb_utils::DateTime;
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use sha2::Sha256;
use tokio::sync::broadcast;

use super::support::{BinanceBalances, BinanceOrderInfo};
use crate::support::BinanceAccountInfo;
use mmb_core::exchanges::common::{
    ActivePosition, Amount, ExchangeError, ExchangeErrorType, Price,
};
use mmb_core::exchanges::events::{
    ExchangeBalance, ExchangeBalancesAndPositions, ExchangeEvent, TradeId,
};
use mmb_core::exchanges::general::features::{
    OrderFeatures, OrderTradeOption, RestFillsFeatures, RestFillsType, WebSocketOptions,
};
use mmb_core::exchanges::general::order::get_order_trades::OrderTrade;
use mmb_core::exchanges::general::symbol::{Precision, Symbol};
use mmb_core::exchanges::hosts::Hosts;
use mmb_core::exchanges::rest_client::{
    BoxErrorHandler, ErrorHandler, ErrorHandlerData, RestClient,
};
use mmb_core::exchanges::traits::{ExchangeClientBuilderResult, Support};
use mmb_core::exchanges::{
    common::CurrencyCode,
    general::features::{ExchangeFeatures, OpenOrdersType},
    timeouts::requests_timeout_manager_factory::RequestTimeoutArguments,
};
use mmb_core::exchanges::{common::CurrencyId, general::exchange::BoxExchangeClient};
use mmb_core::exchanges::{
    common::{CurrencyPair, ExchangeAccountId, RestRequestOutcome, SpecificCurrencyPair},
    events::AllowedEventSourceType,
};
use mmb_core::exchanges::{general::handlers::handle_order_filled::FillEventData, rest_client};
use mmb_core::lifecycle::app_lifetime_manager::AppLifetimeManager;
use mmb_core::orders::fill::EventSourceType;
use mmb_core::orders::order::*;
use mmb_core::orders::pool::OrderRef;
use mmb_core::settings::ExchangeSettings;
use mmb_core::{exchanges::traits::ExchangeClientBuilder, orders::fill::OrderFillType};
use mmb_utils::value_to_decimal::GetOrErr;
use serde::{Deserialize, Serialize};

pub struct ErrorHandlerBinance;

impl ErrorHandlerBinance {
    pub fn new() -> BoxErrorHandler {
        Box::new(ErrorHandlerBinance {})
    }
}

impl ErrorHandler for ErrorHandlerBinance {
    fn check_spec_rest_error(&self, response: &RestRequestOutcome) -> Result<(), ExchangeError> {
        //Binance is a little inconsistent: for failed responses sometimes they include
        //only code or only success:false but sometimes both
        if !(response.content.contains(r#""success":false"#)
            || response.content.contains(r#""code""#))
        {
            return Ok(());
        }

        let data: Value = serde_json::from_str(&response.content)
            .map_err(|err| ExchangeError::parsing_error(&format!("response.content: {:?}", err)))?;

        let message = data["msg"]
            .as_str()
            .ok_or_else(|| ExchangeError::parsing_error("`msg` field"))?;

        let code = data["code"]
            .as_i64()
            .ok_or_else(|| ExchangeError::parsing_error("`code` field"))?;

        Err(ExchangeError::new(
            ExchangeErrorType::Unknown,
            message.to_string(),
            Some(code),
        ))
    }

    fn clarify_error_type(&self, error: &mut ExchangeError) {
        // -1010 ERROR_MSG_RECEIVED
        // -2010 NEW_ORDER_REJECTED
        // -2011 CANCEL_REJECTED
        let error_type = match error.message.as_str() {
            "Unknown order sent." | "Order does not exist." => ExchangeErrorType::OrderNotFound,
            "Account has insufficient balance for requested action." => {
                ExchangeErrorType::InsufficientFunds
            }
            "Invalid quantity."
            | "Filter failure: MIN_NOTIONAL"
            | "Filter failure: LOT_SIZE"
            | "Filter failure: PRICE_FILTER"
            | "Filter failure: PERCENT_PRICE"
            | "Quantity less than zero."
            | "Precision is over the maximum defined for this asset." => {
                ExchangeErrorType::InvalidOrder
            }
            msg if msg.contains("Too many requests;") => ExchangeErrorType::RateLimit,
            _ => ExchangeErrorType::Unknown,
        };

        error.error_type = error_type;
    }
}

pub struct Binance {
    pub settings: ExchangeSettings,
    pub hosts: Hosts,
    pub id: ExchangeAccountId,
    pub order_created_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>>,
    pub order_cancelled_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>>,
    pub handle_order_filled_callback: Mutex<Box<dyn FnMut(FillEventData) + Send + Sync>>,
    pub handle_trade_callback: Mutex<
        Box<dyn FnMut(CurrencyPair, TradeId, Price, Amount, OrderSide, DateTime) + Send + Sync>,
    >,

    pub unified_to_specific: RwLock<HashMap<CurrencyPair, SpecificCurrencyPair>>,
    pub specific_to_unified: RwLock<HashMap<SpecificCurrencyPair, CurrencyPair>>,
    pub supported_currencies: DashMap<CurrencyId, CurrencyCode>,
    // Currencies used for trading according to user settings
    pub traded_specific_currencies: Mutex<Vec<SpecificCurrencyPair>>,
    pub(super) last_trade_ids: DashMap<CurrencyPair, TradeId>,

    pub(super) lifetime_manager: Arc<AppLifetimeManager>,

    pub(super) events_channel: broadcast::Sender<ExchangeEvent>,

    pub(super) subscribe_to_market_data: bool,
    pub(super) is_reducing_market_data: bool,

    pub(super) rest_client: RestClient,
}

impl Binance {
    pub fn new(
        id: ExchangeAccountId,
        settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
        is_reducing_market_data: bool,
        empty_response_is_ok: bool,
    ) -> Self {
        let is_reducing_market_data = settings
            .is_reducing_market_data
            .unwrap_or(is_reducing_market_data);

        let hosts = Self::make_hosts(settings.is_margin_trading);
        let exchange_account_id = settings.exchange_account_id;

        Self {
            id,
            order_created_callback: Mutex::new(Box::new(|_, _, _| {})),
            order_cancelled_callback: Mutex::new(Box::new(|_, _, _| {})),
            handle_order_filled_callback: Mutex::new(Box::new(|_| {})),
            handle_trade_callback: Mutex::new(Box::new(|_, _, _, _, _, _| {})),
            unified_to_specific: Default::default(),
            specific_to_unified: Default::default(),
            supported_currencies: Default::default(),
            traded_specific_currencies: Default::default(),
            last_trade_ids: Default::default(),
            subscribe_to_market_data: settings.subscribe_to_market_data,
            is_reducing_market_data,
            settings,
            hosts,
            events_channel,
            lifetime_manager,
            rest_client: RestClient::new(ErrorHandlerData::new(
                empty_response_is_ok,
                exchange_account_id,
                ErrorHandlerBinance::new(),
            )),
        }
    }

    pub fn make_hosts(is_margin_trading: bool) -> Hosts {
        if is_margin_trading {
            Hosts {
                web_socket_host: "wss://fstream.binance.com",
                web_socket2_host: "wss://fstream3.binance.com",
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

    pub(super) async fn get_listen_key(&self) -> Result<RestRequestOutcome> {
        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/sapi/v1/userDataStream", "/api/v3/userDataStream"),
            &vec![],
        )?;
        let http_params = rest_client::HttpParams::new();

        self.rest_client
            .post(
                full_url,
                &self.settings.api_key,
                &http_params,
                "get_listen_key",
                "".to_string(),
            )
            .await
    }

    // TODO Change to pub(super) or pub(crate) after implementation if possible
    pub async fn reconnect(&mut self) {
        todo!("reconnect")
    }

    pub(super) fn get_stream_name(
        specific_currency_pair: &SpecificCurrencyPair,
        channel: &str,
    ) -> String {
        format!("{}@{}", specific_currency_pair.as_str(), channel)
    }

    fn _is_websocket_reconnecting(&self) -> bool {
        todo!("is_websocket_reconnecting")
    }

    pub(super) fn to_server_order_side(side: OrderSide) -> String {
        match side {
            OrderSide::Buy => "BUY".to_owned(),
            OrderSide::Sell => "SELL".to_owned(),
        }
    }

    pub(super) fn to_local_order_side(side: &str) -> OrderSide {
        match side {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            // TODO just propagate and log there
            _ => panic!("Unexpected order side"),
        }
    }

    fn to_local_order_status(status: &str) -> OrderStatus {
        match status {
            "NEW" | "PARTIALLY_FILLED" => OrderStatus::Created,
            "FILLED" => OrderStatus::Completed,
            "PENDING_CANCEL" => OrderStatus::Canceling,
            "CANCELED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
            // TODO just propagate and log there
            _ => panic!("Unexpected order status"),
        }
    }

    pub(super) fn to_server_order_type(order_type: OrderType) -> String {
        match order_type {
            OrderType::Limit => "LIMIT".to_owned(),
            OrderType::Market => "MARKET".to_owned(),
            unexpected_variant => panic!("{:?} are not expected", unexpected_variant),
        }
    }

    fn generate_signature(&self, data: String) -> Result<String> {
        let mut hmac = Hmac::<Sha256>::new_from_slice(self.settings.secret_key.as_bytes())
            .context("Unable to calculate hmac")?;
        hmac.update(data.as_bytes());
        let result = hex::encode(&hmac.finalize().into_bytes());

        return Ok(result);
    }

    pub(super) fn add_authentification_headers(
        &self,
        parameters: &mut rest_client::HttpParams,
    ) -> Result<()> {
        let time_stamp = get_current_milliseconds();
        parameters.push(("timestamp".to_owned(), time_stamp.to_string()));

        let message_to_sign = rest_client::to_http_string(&parameters);
        let signature = self.generate_signature(message_to_sign)?;
        parameters.push(("signature".to_owned(), signature));

        Ok(())
    }

    pub(super) fn get_unified_currency_pair(
        &self,
        currency_pair: &SpecificCurrencyPair,
    ) -> Result<CurrencyPair> {
        self.specific_to_unified
            .read()
            .get(currency_pair)
            .with_context(|| {
                format!(
                    "Not found currency pair '{:?}' in {}",
                    currency_pair, self.id
                )
            })
            .map(Clone::clone)
    }

    pub(super) fn specific_order_info_to_unified(&self, specific: &BinanceOrderInfo) -> OrderInfo {
        OrderInfo::new(
            self.get_unified_currency_pair(&specific.specific_currency_pair)
                .expect("expected known currency pair"),
            specific.exchange_order_id.to_string().as_str().into(),
            specific.client_order_id.clone(),
            Self::to_local_order_side(&specific.side),
            Self::to_local_order_status(&specific.status),
            specific.price,
            specific.orig_quantity,
            specific.price,
            specific.executed_quantity,
            None,
            None,
            None,
        )
    }

    pub(super) fn handle_order_fill(&self, msg_to_log: &str, json_response: Value) -> Result<()> {
        let original_client_order_id = json_response["C"]
            .as_str()
            .ok_or(anyhow!("Unable to parse original client order id"))?;

        let client_order_id = if original_client_order_id.is_empty() {
            json_response["c"]
                .as_str()
                .ok_or(anyhow!("Unable to parse client order id"))?
        } else {
            original_client_order_id
        };

        let exchange_order_id = json_response["i"].to_string();
        let exchange_order_id = exchange_order_id.trim_matches('"');
        let execution_type = json_response["x"]
            .as_str()
            .ok_or(anyhow!("Unable to parse execution type"))?;
        let order_status = json_response["X"]
            .as_str()
            .ok_or(anyhow!("Unable to parse order status"))?;
        let time_in_force = json_response["f"]
            .as_str()
            .ok_or(anyhow!("Unable to parse time in force"))?;

        match execution_type {
            "NEW" => match order_status {
                "NEW" => {
                    (&self.order_created_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => log::error!(
                    "execution_type is NEW but order_status is {} for message {}",
                    order_status,
                    msg_to_log
                ),
            },
            "CANCELED" => match order_status {
                "CANCELED" => {
                    (&self.order_cancelled_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => log::error!(
                    "execution_type is CANCELED but order_status is {} for message {}",
                    order_status,
                    msg_to_log
                ),
            },
            "REJECTED" => {
                // TODO: May be not handle error in Rest but move it here to make it unified?
                // We get notification of rejected orders from the rest responses
            }
            "EXPIRED" => match time_in_force {
                "GTX" => {
                    (&self.order_cancelled_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => log::error!(
                    "Order {} was expired, message: {}",
                    client_order_id,
                    msg_to_log
                ),
            },
            "TRADE" | "CALCULATED" => {
                let event_data = self.prepare_data_for_fill_handler(
                    &json_response,
                    execution_type,
                    client_order_id.into(),
                    exchange_order_id.into(),
                )?;

                (&self.handle_order_filled_callback).lock()(event_data);
            }
            _ => log::error!("Impossible execution type"),
        }

        Ok(())
    }

    pub(crate) fn get_currency_code(&self, currency_id: &CurrencyId) -> Option<CurrencyCode> {
        self.supported_currencies
            .get(currency_id)
            .map(|some| some.value().clone())
    }

    pub(crate) fn get_currency_code_expected(&self, currency_id: &CurrencyId) -> CurrencyCode {
        self.get_currency_code(currency_id).with_expect(|| {
            format!(
                "Failed to convert CurrencyId({}) to CurrencyCode for {}",
                currency_id, self.id
            )
        })
    }

    fn prepare_data_for_fill_handler(
        &self,
        json_response: &Value,
        execution_type: &str,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
    ) -> Result<FillEventData> {
        let trade_id = json_response["t"].clone().into();
        let last_filled_price = json_response["L"]
            .as_str()
            .ok_or(anyhow!("Unable to parse last filled price"))?;
        let last_filled_amount = json_response["l"]
            .as_str()
            .ok_or(anyhow!("Unable to parse last filled amount"))?;
        let total_filled_amount = json_response["z"]
            .as_str()
            .ok_or(anyhow!("Unable to parse total filled amount"))?;
        let commission_amount = json_response["n"]
            .as_str()
            .ok_or(anyhow!("Unable to parse last commission amount"))?;
        let commission_currency = json_response["N"]
            .as_str()
            .ok_or(anyhow!("Unable to parse last commission currency"))?;
        let commission_currency_code = self
            .get_currency_code(&commission_currency.into())
            .ok_or(anyhow!("There are no suck supported currency code"))?;
        let is_maker = json_response["m"]
            .as_bool()
            .ok_or(anyhow!("Unable to parse trade side"))?;
        let order_side = Self::to_local_order_side(
            json_response["S"]
                .as_str()
                .ok_or(anyhow!("Unable to parse last filled amount"))?,
        );
        let fill_date: DateTime = u64_to_date_time(
            json_response["E"]
                .as_u64()
                .ok_or(anyhow!("Unable to parse transaction time"))?,
        );

        let fill_type = Self::get_fill_type(execution_type)?;
        let order_role = if is_maker {
            OrderRole::Maker
        } else {
            OrderRole::Taker
        };

        let event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id: Some(trade_id),
            client_order_id: Some(client_order_id),
            exchange_order_id,
            fill_price: last_filled_price.parse()?,
            fill_amount: last_filled_amount.parse()?,
            is_diff: true,
            total_filled_amount: Some(total_filled_amount.parse()?),
            order_role: Some(order_role),
            commission_currency_code: Some(commission_currency_code),
            commission_rate: None,
            commission_amount: Some(commission_amount.parse()?),
            fill_type,
            trade_currency_pair: None,
            order_side: Some(order_side),
            order_amount: None,
            fill_date: Some(fill_date),
        };

        Ok(event_data)
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
        raw_balances: Vec<BinanceBalances>,
    ) -> ExchangeBalancesAndPositions {
        let balances = raw_balances
            .iter()
            .map(|balance| ExchangeBalance {
                currency_code: self.get_currency_code_expected(&balance.asset.as_str().into()),
                balance: balance.free,
            })
            .collect_vec();

        ExchangeBalancesAndPositions {
            balances,
            positions: None,
        }
    }

    pub(super) fn get_margin_exchange_balances_and_positions(
        _raw_balances: Vec<BinanceBalances>,
    ) -> ExchangeBalancesAndPositions {
        todo!("implement it later")
    }

    pub(super) fn get_order_id(&self, response: &RestRequestOutcome) -> Result<ExchangeOrderId> {
        let deserialized: OrderId = serde_json::from_str(&response.content)
            .expect("Unable to parse orderId from response content");

        Ok(ExchangeOrderId::new(
            deserialized.order_id.to_string().into(),
        ))
    }

    pub(super) fn get_url_path<'a>(
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
        http_params: Vec<(String, String)>,
    ) -> Result<RestRequestOutcome> {
        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/fapi/v1/openOrders", "/api/v3/openOrders"),
            &http_params,
        )?;

        self.rest_client
            .get(
                full_url,
                &self.settings.api_key,
                function_name!(),
                "".to_string(),
            )
            .await
    }

    #[named]
    pub(super) async fn request_order_info(&self, order: &OrderRef) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(order.currency_pair());

        let mut http_params = vec![
            (
                "symbol".to_owned(),
                specific_currency_pair.as_str().to_owned(),
            ),
            (
                "origClientOrderId".to_owned(),
                order.client_order_id().as_str().to_owned(),
            ),
        ];
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/fapi/v1/order", "/api/v3/order"),
            &http_params,
        )?;

        let order_header = order.fn_ref(|order| order.header.clone());

        let log_args = format_args!("order {}", order_header.client_order_id).to_string();

        self.rest_client
            .get(full_url, &self.settings.api_key, function_name!(), log_args)
            .await
    }

    pub(super) fn parse_order_info(&self, response: &RestRequestOutcome) -> OrderInfo {
        let specific_order: BinanceOrderInfo = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_order_info request");

        self.specific_order_info_to_unified(&specific_order)
    }

    pub(super) async fn request_open_orders(&self) -> Result<RestRequestOutcome> {
        let mut http_params = rest_client::HttpParams::new();
        self.add_authentification_headers(&mut http_params)?;

        self.request_open_orders_by_http_header(http_params).await
    }

    pub(super) async fn request_open_orders_by_currency_pair(
        &self,
        currency_pair: CurrencyPair,
    ) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(currency_pair);
        let mut http_params = vec![(
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        )];
        self.add_authentification_headers(&mut http_params)?;

        self.request_open_orders_by_http_header(http_params).await
    }

    pub(super) fn parse_open_orders(&self, response: &RestRequestOutcome) -> Vec<OrderInfo> {
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
    ) -> Result<RestRequestOutcome> {
        let side = match position.derivative.side {
            Some(side) => side.change_side().to_string(),
            None => "0".to_string(), // unknown side
        };

        let mut http_params = vec![
            (
                "leverage".to_string(),
                position.derivative.leverage.to_string(),
            ),
            ("positionSide".to_string(), "BOTH".to_string()),
            (
                "quantity".to_string(),
                position.derivative.position.abs().to_string(),
            ),
            ("side".to_string(), side),
            (
                "symbol".to_string(),
                position.derivative.currency_pair.to_string(),
            ),
        ];

        match price {
            Some(price) => {
                http_params.push(("type".to_string(), "MARKET".to_string()));
                http_params.push(("price".to_string(), price.to_string()));
            }
            None => http_params.push(("type".to_string(), "LIMIT".to_string())),
        }

        self.add_authentification_headers(&mut http_params)?;

        let url_path = "/fapi/v1/order";
        let full_url = rest_client::build_uri(&self.hosts.rest_host, url_path, &http_params)?;

        let log_args =
            format_args!("Close position response for {:?} {:?}", position, price).to_string();

        self.rest_client
            .post(
                full_url,
                &self.settings.api_key,
                &http_params,
                function_name!(),
                log_args,
            )
            .await
    }

    #[named]
    pub(super) async fn request_get_position(&self) -> Result<RestRequestOutcome> {
        let mut http_params = Vec::new();
        self.add_authentification_headers(&mut http_params)?;

        let url_path = "/fapi/v2/positionRisk";
        let full_url = rest_client::build_uri(&self.hosts.rest_host, url_path, &http_params)?;

        self.rest_client
            .get(
                full_url,
                &self.settings.api_key,
                function_name!(),
                "".to_string(),
            )
            .await
    }

    #[named]
    pub(super) async fn request_get_balance(&self) -> Result<RestRequestOutcome> {
        let mut http_params = Vec::new();
        self.add_authentification_headers(&mut http_params)?;
        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/fapi/v2/account", "/api/v3/account"),
            &http_params,
        )?;

        self.rest_client
            .get(
                full_url,
                &self.settings.api_key,
                function_name!(),
                "".to_string(),
            )
            .await
    }

    pub(super) async fn request_get_balance_and_position(&self) -> Result<RestRequestOutcome> {
        panic!("not supported request")
    }

    pub(super) fn parse_get_balance(
        &self,
        response: &RestRequestOutcome,
    ) -> ExchangeBalancesAndPositions {
        let binance_account_info: BinanceAccountInfo = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_balance request");

        if self.settings.is_margin_trading {
            Binance::get_margin_exchange_balances_and_positions(binance_account_info.balances)
        } else {
            self.get_spot_exchange_balances_and_positions(binance_account_info.balances)
        }
    }

    #[named]
    pub(super) async fn request_cancel_order(
        &self,
        order: OrderCancelling,
    ) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(order.header.currency_pair);

        let mut http_params = vec![
            (
                "symbol".to_owned(),
                specific_currency_pair.as_str().to_owned(),
            ),
            (
                "orderId".to_owned(),
                order.exchange_order_id.as_str().to_owned(),
            ),
        ];
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/fapi/v1/order", "/api/v3/order"),
            &http_params,
        )?;

        let log_args =
            format_args!("Cancel order for {}", order.header.client_order_id).to_string();
        self.rest_client
            .delete(full_url, &self.settings.api_key, function_name!(), log_args)
            .await
    }

    #[named]
    pub(super) async fn request_my_trades(
        &self,
        symbol: &Symbol,
        _last_date_time: Option<DateTime>,
    ) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(symbol.currency_pair());
        let mut http_params = vec![(
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        )];

        self.add_authentification_headers(&mut http_params)?;
        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/fapi/v1/userTrades", "/api/v3/myTrades"),
            &http_params,
        )?;

        self.rest_client
            .get(
                full_url,
                &self.settings.api_key,
                function_name!(),
                "".to_string(),
            )
            .await
    }

    pub(super) fn parse_get_my_trades(
        &self,
        response: &RestRequestOutcome,
        _last_date_time: Option<DateTime>,
    ) -> Result<Vec<OrderTrade>> {
        #[derive(Serialize, Deserialize, Debug)]
        #[serde(rename_all = "camelCase")]
        struct BinanceMyTrade {
            id: TradeId,
            order_id: u64,
            price: Price,
            #[serde(alias = "qty")]
            amount: Amount,
            commission: Amount,
            #[serde(alias = "commissionAsset")]
            commission_currency_code: CurrencyId,
            time: u64,
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
                    ExchangeOrderId::from(self.order_id.to_string().as_ref()),
                    self.id.clone(),
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
        order: OrderCreating,
    ) -> Result<RestRequestOutcome> {
        let specific_currency_pair = self.get_specific_currency_pair(order.header.currency_pair);

        let mut http_params = vec![
            (
                "symbol".to_owned(),
                specific_currency_pair.as_str().to_owned(),
            ),
            (
                "side".to_owned(),
                Self::to_server_order_side(order.header.side),
            ),
            (
                "type".to_owned(),
                Self::to_server_order_type(order.header.order_type),
            ),
            ("quantity".to_owned(), order.header.amount.to_string()),
            (
                "newClientOrderId".to_owned(),
                order.header.client_order_id.as_str().to_owned(),
            ),
        ];

        if order.header.order_type != OrderType::Market {
            http_params.push(("timeInForce".to_owned(), "GTC".to_owned()));
            http_params.push(("price".to_owned(), order.price.to_string()));
        } else if order.header.execution_type == OrderExecutionType::MakerOnly {
            http_params.push(("timeInForce".to_owned(), "GTX".to_owned()));
        }
        self.add_authentification_headers(&mut http_params)?;

        let full_url = rest_client::build_uri(
            &self.hosts.rest_host,
            self.get_url_path("/fapi/v1/order", "/api/v3/order"),
            &vec![],
        )?;

        let log_args = format_args!(
            "Create order for {}",
            // TODO other order_headers_field
            order.header.client_order_id,
        )
        .to_string();

        self.rest_client
            .post(
                full_url,
                &self.settings.api_key,
                &http_params,
                function_name!(),
                log_args,
            )
            .await
    }

    #[named]
    pub(super) async fn request_all_symbols(&self) -> Result<RestRequestOutcome> {
        // In current versions works only with Spot market
        let url_path = "/api/v3/exchangeInfo";
        let full_url = rest_client::build_uri(&self.hosts.rest_host, url_path, &vec![])?;

        self.rest_client
            .get(
                full_url,
                &self.settings.api_key,
                function_name!(),
                "".to_string(),
            )
            .await
    }

    pub(super) fn parse_all_symbols(
        &self,
        response: &RestRequestOutcome,
    ) -> Result<Vec<Arc<Symbol>>> {
        let deserialized: Value = serde_json::from_str(&response.content)
            .expect("Unable to deserialize response from Binance");
        let symbols = deserialized
            .get("symbols")
            .and_then(|symbols| symbols.as_array())
            .ok_or(anyhow!("Unable to get symbols array from Binance"))?;

        let mut result = Vec::new();
        for symbol in symbols {
            let is_active = symbol["status"] == "TRADING";

            // TODO There is no work with derivatives in current version
            let is_derivative = false;
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

            let amount_currency_code = base;

            // TODO There are no balance_currency_code for spot, why does it set here this way?
            let balance_currency_code = base;

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
                        min_cost = filter.get_as_decimal("minNotional");
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
                is_active,
                is_derivative,
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
                Some(balance_currency_code),
                price_precision,
                amount_precision,
            );

            result.push(Arc::new(symbol))
        }

        Ok(result)
    }
}

pub struct BinanceBuilder;

impl ExchangeClientBuilder for BinanceBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
        events_channel: broadcast::Sender<ExchangeEvent>,
        lifetime_manager: Arc<AppLifetimeManager>,
    ) -> ExchangeClientBuilderResult {
        let exchange_account_id = exchange_settings.exchange_account_id;
        let empty_response_is_ok = false;

        ExchangeClientBuilderResult {
            client: Box::new(Binance::new(
                exchange_account_id,
                exchange_settings,
                events_channel.clone(),
                lifetime_manager,
                false,
                empty_response_is_ok,
            )) as BoxExchangeClient,
            features: ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                RestFillsFeatures::new(RestFillsType::None),
                OrderFeatures::default(),
                OrderTradeOption::default(),
                WebSocketOptions::default(),
                empty_response_is_ok,
                false,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        }
    }

    fn get_timeout_arguments(&self) -> RequestTimeoutArguments {
        RequestTimeoutArguments::from_requests_per_minute(1200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmb_utils::cancellation_token::CancellationToken;

    #[test]
    fn generate_signature() {
        // All values and strings gotten from binanсe API example
        let right_value = "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71";

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
            false,
            false,
        );
        let params = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559".into();
        let result = binance.generate_signature(params).expect("in test");
        assert_eq!(result, right_value);
    }

    #[test]
    fn to_http_string() {
        let parameters: rest_client::HttpParams = vec![
            ("symbol".to_owned(), "LTCBTC".to_owned()),
            ("side".to_owned(), "BUY".to_owned()),
            ("type".to_owned(), "LIMIT".to_owned()),
            ("timeInForce".to_owned(), "GTC".to_owned()),
            ("quantity".to_owned(), "1".to_owned()),
            ("price".to_owned(), "0.1".to_owned()),
            ("recvWindow".to_owned(), "5000".to_owned()),
            ("timestamp".to_owned(), "1499827319559".to_owned()),
        ];

        let http_string = rest_client::to_http_string(&parameters);

        let right_value = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
        assert_eq!(http_string, right_value);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderId {
    order_id: u64,
}
