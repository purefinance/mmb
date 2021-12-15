use crate::core::misc::derivative_position::DerivativePosition;
use mmb_utils::infrastructure::WithExpect;
use std::str::FromStr;

use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use awc::http::Uri;
use chrono::{TimeZone, Utc};
use dashmap::DashMap;
use itertools::Itertools;
use mmb_utils::DateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::binance::Binance;
use crate::core::exchanges::common::{ActivePosition, ClosedPosition, SortedOrderData};
use crate::core::exchanges::events::{ExchangeBalancesAndPositions, ExchangeEvent, TradeId};
use crate::core::exchanges::general::order::get_order_trades::OrderTrade;
use crate::core::exchanges::rest_client;
use crate::core::exchanges::{
    common::CurrencyCode, common::CurrencyId,
    general::handlers::handle_order_filled::FillEventData, general::symbol::Symbol,
    traits::Support,
};
use crate::core::order_book::event::{EventType, OrderBookEvent};
use crate::core::order_book::order_book_data::OrderBookData;
use crate::core::orders::fill::OrderFillType;
use crate::core::orders::order::*;
use crate::core::settings::ExchangeSettings;
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole, exchanges::general::symbol::Precision,
};
use crate::core::{
    exchanges::common::{
        Amount, CurrencyPair, ExchangeError, ExchangeErrorType, Price, RestRequestOutcome,
        SpecificCurrencyPair,
    },
    orders::fill::EventSourceType,
};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BinanceOrderInfo {
    #[serde(rename = "symbol")]
    pub specific_currency_pair: SpecificCurrencyPair,
    #[serde(rename = "orderId")]
    pub exchange_order_id: i64, //< local type is ExchangeOrderId
    #[serde(rename = "clientOrderId")]
    pub client_order_id: ClientOrderId,
    pub price: Price,
    #[serde(rename = "origQty")]
    pub orig_quantity: Amount,
    #[serde(rename = "executedQty")]
    pub executed_quantity: Amount,
    pub status: String,
    pub side: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BinanceAccountInfo {
    pub balances: Vec<BinanceBalances>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BinanceBalances {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
struct BinancePosition {
    #[serde(rename = "symbol")]
    pub specific_currency_pair: SpecificCurrencyPair,
    #[serde(rename = "PositionAmt")]
    pub position_amount: Amount,
    #[serde(rename = "LiquidationPrice")]
    pub liquidation_price: Price,
    pub leverage: Decimal,
    #[serde(rename = "PositionSide")]
    pub position_side: Decimal,
}

#[async_trait]
impl Support for Binance {
    fn is_rest_error_code(&self, response: &RestRequestOutcome) -> Result<(), ExchangeError> {
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

    fn get_order_id(&self, response: &RestRequestOutcome) -> Result<ExchangeOrderId> {
        let response: Value =
            serde_json::from_str(&response.content).context("Unable to parse response content")?;
        let id = response["orderId"].to_string();
        let id = id.trim_matches('"');
        Ok(ExchangeOrderId::new(id.into()))
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

    fn on_websocket_message(&self, msg: &str) -> Result<()> {
        let mut data: Value =
            serde_json::from_str(msg).context("Unable to parse websocket message")?;
        // Public stream
        if let Some(stream) = data.get("stream") {
            let stream = stream
                .as_str()
                .ok_or(anyhow!("Unable to parse stream data"))?;

            if let Some(byte_index) = stream.find('@') {
                let currency_pair = self.currency_pair_from_web_socket(&stream[..byte_index])?;
                let data = &data["data"];

                if stream.ends_with("@trade") {
                    self.handle_trade(currency_pair, data)?;
                    return Ok(());
                }

                // TODO handle public stream
                if stream.ends_with("depth20") {
                    self.process_snapshot_update(currency_pair, data)?;
                    return Ok(());
                }
            }

            return Ok(());
        }

        // so it is userData stream
        let event_type = data["e"]
            .as_str()
            .ok_or(anyhow!("Unable to parse event_type"))?;
        if event_type == "executionReport" {
            self.handle_order_fill(msg, data)?;
        } else if event_type == "ORDER_TRADE_UPDATE" {
            let json_response = data["o"].take();
            self.handle_order_fill(msg, json_response)?;
        } else {
            self.log_unknown_message(self.id, msg);
        }

        Ok(())
    }

    fn on_connecting(&self) -> Result<()> {
        self.unified_to_specific
            .read()
            .iter()
            .for_each(|(currency_pair, _)| {
                let _ = self
                    .last_trade_ids
                    .insert(currency_pair.clone(), TradeId::Number(0));
            });

        Ok(())
    }

    fn set_order_created_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    ) {
        *self.order_created_callback.lock() = callback;
    }

    fn set_order_cancelled_callback(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>,
    ) {
        *self.order_cancelled_callback.lock() = callback;
    }

    fn set_handle_order_filled_callback(
        &self,
        callback: Box<dyn FnMut(FillEventData) + Send + Sync>,
    ) {
        *self.handle_order_filled_callback.lock() = callback;
    }

    fn set_handle_trade_callback(
        &self,
        callback: Box<
            dyn FnMut(CurrencyPair, TradeId, Price, Amount, OrderSide, DateTime) + Send + Sync,
        >,
    ) {
        *self.handle_trade_callback.lock() = callback;
    }

    fn set_traded_specific_currencies(&self, currencies: Vec<SpecificCurrencyPair>) {
        *self.traded_specific_currencies.lock() = currencies;
    }

    fn is_websocket_enabled(&self, role: WebSocketRole) -> bool {
        match role {
            WebSocketRole::Main => true,
            WebSocketRole::Secondary => {
                self.settings.api_key != "" && self.settings.secret_key != ""
            }
        }
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Uri> {
        let (host, path) = match role {
            WebSocketRole::Main => (
                &self.hosts.web_socket_host,
                self.build_ws_main_path(&self.settings.websocket_channels[..]),
            ),
            WebSocketRole::Secondary => (
                &self.hosts.web_socket2_host,
                self.build_ws_secondary_path().await?,
            ),
        };

        format!("{}{}", host, path)
            .parse::<Uri>()
            .with_context(|| format!("Unable parse websocket {:?} uri", role))
    }

    fn get_specific_currency_pair(&self, currency_pair: CurrencyPair) -> SpecificCurrencyPair {
        self.unified_to_specific.read()[&currency_pair]
    }

    fn get_supported_currencies(&self) -> &DashMap<CurrencyId, CurrencyCode> {
        &self.supported_currencies
    }

    fn should_log_message(&self, message: &str) -> bool {
        message.contains("executionReport")
    }

    fn log_unknown_message(
        &self,
        exchange_account_id: crate::core::exchanges::common::ExchangeAccountId,
        message: &str,
    ) {
        log::info!("Unknown message for {}: {}", exchange_account_id, message);
    }

    fn parse_open_orders(&self, response: &RestRequestOutcome) -> Result<Vec<OrderInfo>> {
        let binance_orders: Vec<BinanceOrderInfo> = serde_json::from_str(&response.content)
            .context("Unable to parse response content for get_open_orders request")?;

        let orders_info: Vec<OrderInfo> = binance_orders
            .iter()
            .map(|order| self.specific_order_info_to_unified(order))
            .collect();

        Ok(orders_info)
    }

    fn parse_order_info(&self, response: &RestRequestOutcome) -> Result<OrderInfo> {
        let specific_order: BinanceOrderInfo = serde_json::from_str(&response.content)
            .context("Unable to parse response content for get_order_info request")?;
        let unified_order = self.specific_order_info_to_unified(&specific_order);

        Ok(unified_order)
    }

    fn parse_all_symbols(&self, response: &RestRequestOutcome) -> Result<Vec<Arc<Symbol>>> {
        let deserialized: Value = serde_json::from_str(&response.content)
            .context("Unable to deserialize response from Binance")?;
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
                .context("Unable to get base currency id from Binance")?;
            let quote_currency_id = &symbol
                .get_as_str("quoteAsset")
                .context("Unable to get quote currency id from Binance")?;
            let base = base_currency_id.as_str().into();
            let quote = quote_currency_id.as_str().into();

            let specific_currency_pair = symbol.get_as_str("symbol")?.as_str().into();
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
                .context("Unable to get filters as array from Binance")?;
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

    fn parse_get_my_trades(
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
            pub(super) fn to_unified_order_trade(
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

        let my_trades: Vec<BinanceMyTrade> = serde_json::from_str(&response.content)?;

        my_trades
            .into_iter()
            .map(|my_trade| {
                my_trade.to_unified_order_trade(
                    self.get_currency_code(&my_trade.commission_currency_code),
                )
            })
            .collect()
    }

    fn get_settings(&self) -> &ExchangeSettings {
        &self.settings
    }

    fn parse_get_position(&self, response: &RestRequestOutcome) -> Vec<ActivePosition> {
        let binance_positions: Vec<BinancePosition> = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_active_positions_core request");

        binance_positions
            .into_iter()
            .map(|x| self.binance_position_to_active_position(x))
            .collect_vec()
    }

    fn parse_close_position(&self, response: &RestRequestOutcome) -> Result<ClosedPosition> {
        let binance_order: BinanceOrderInfo = serde_json::from_str(&response.content)
            .context("Unable to parse response content for get_open_orders request")?;

        let closed_position = ClosedPosition::new(
            ExchangeOrderId::from(binance_order.exchange_order_id.to_string().as_ref()),
            binance_order.orig_quantity,
        );

        Ok(closed_position)
    }

    fn parse_get_balance(&self, response: &RestRequestOutcome) -> ExchangeBalancesAndPositions {
        let binance_account_info: BinanceAccountInfo = serde_json::from_str(&response.content)
            .expect("Unable to parse response content for get_balance request");

        if self.settings.is_margin_trading {
            Binance::get_margin_exchange_balances_and_positions(binance_account_info.balances)
        } else {
            self.get_spot_exchange_balances_and_positions(binance_account_info.balances)
        }
    }
}

trait GetOrErr {
    fn get_as_str(&self, key: &str) -> Result<String>;
    fn get_as_decimal(&self, key: &str) -> Option<Decimal>;
}

impl GetOrErr for Value {
    fn get_as_str(&self, key: &str) -> Result<String> {
        Ok(self
            .get(key)
            .with_context(|| format!("Unable to get {} from Binance", key))?
            .as_str()
            .with_context(|| format!("Unable to get {} as string from Binance", key))?
            .to_string())
    }

    fn get_as_decimal(&self, key: &str) -> Option<Decimal> {
        self.get(key)
            .and_then(|value| value.as_str())
            .and_then(|value| Decimal::from_str(value).ok())
    }
}

impl Binance {
    pub(crate) fn handle_trade(&self, currency_pair: CurrencyPair, data: &Value) -> Result<()> {
        let trade_id = TradeId::from(data["t"].clone());

        let mut trade_id_from_lasts =
            self.last_trade_ids.get_mut(&currency_pair).with_expect(|| {
                format!(
                    "There are no last_trade_id for given currency_pair {}",
                    currency_pair
                )
            });

        if self.is_reducing_market_data && trade_id_from_lasts.get_number() >= trade_id.get_number()
        {
            log::info!(
                "Current last_trade_id for currency_pair {} is {} >= trade_id {}",
                currency_pair,
                *trade_id_from_lasts,
                trade_id
            );

            return Ok(());
        }

        *trade_id_from_lasts = trade_id.clone();

        let price: Decimal = data["p"]
            .as_str()
            .context("Unable to get string from 'p' field json data")?
            .parse()?;

        let quantity: Decimal = data["q"]
            .as_str()
            .context("Unable to get string from 'q' field json data")?
            .parse()?;
        let order_side = if data["m"] == true {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        let datetime = data["T"]
            .as_i64()
            .context("Unable to get i64 from 'T' field json data")?;

        (&self.handle_trade_callback).lock()(
            currency_pair,
            trade_id,
            price,
            quantity,
            order_side,
            Utc.timestamp_millis(datetime),
        );

        Ok(())
    }

    pub fn process_snapshot_update(&self, currency_pair: CurrencyPair, data: &Value) -> Result<()> {
        let last_update_id = data["lastUpdateId"].to_string();
        let last_update_id = last_update_id.trim_matches('"');
        let raw_asks = data["asks"]
            .as_array()
            .ok_or(anyhow!("Unable to parse 'asks' in Binance"))?;
        let raw_bids = data["bids"]
            .as_array()
            .ok_or(anyhow!("Unable to parse 'bids' in Binance"))?;

        let asks = get_order_book_side(raw_asks)?;
        let bids = get_order_book_side(raw_bids)?;

        let order_book_data = OrderBookData::new(asks, bids);
        self.handle_order_book_snapshot(currency_pair, &last_update_id, order_book_data, None)
    }

    fn handle_order_book_snapshot(
        &self,
        currency_pair: CurrencyPair,
        event_id: &str,
        mut order_book_data: OrderBookData,
        order_book_update: Option<Vec<OrderBookData>>,
    ) -> Result<()> {
        if !self.subscribe_to_market_data {
            return Ok(());
        }

        //Some exchanges like Binance don't give us Snapshot in Web Socket, so we have to request Snapshot using Rest
        //and then update it with orderBookUpdates that we received while Rest request was being executed
        if let Some(updates) = order_book_update {
            order_book_data.update(updates)
        }

        let order_book_event = OrderBookEvent::new(
            Utc::now(),
            self.id,
            currency_pair,
            event_id.to_string(),
            EventType::Snapshot,
            Arc::new(order_book_data),
        );

        let event = ExchangeEvent::OrderBookEvent(order_book_event);

        // TODO safe event in database if needed

        self.send_event(event)
    }

    fn currency_pair_from_web_socket(&self, currency_pair: &str) -> Result<CurrencyPair> {
        let specific_currency_pair = currency_pair.to_uppercase().as_str().into();
        self.get_unified_currency_pair(&specific_currency_pair)
    }

    fn send_event(&self, event: ExchangeEvent) -> Result<()> {
        match self.events_channel.send(event) {
            Ok(_) => Ok(()),
            Err(error) => {
                let msg = format!("Unable to send exchange event in {}: {}", self.id, error);
                log::error!("{}", msg);
                self.application_manager
                    .clone()
                    .spawn_graceful_shutdown(msg.clone());
                Err(anyhow!(msg))
            }
        }
    }

    fn build_ws_main_path(&self, websocket_channels: &[String]) -> String {
        let stream_names = self
            .traded_specific_currencies
            .lock()
            .iter()
            .flat_map(|currency_pair| {
                let mut results = Vec::new();
                for channel in websocket_channels {
                    let result = Self::get_stream_name(currency_pair, channel);
                    results.push(result);
                }
                results
            })
            .join("/");
        let ws_path = format!("/stream?streams={}", stream_names);
        ws_path.to_lowercase()
    }

    async fn build_ws_secondary_path(&self) -> Result<String> {
        let request_outcome = self
            .get_listen_key()
            .await
            .context("Unable to get listen key for Binance")?;
        let data: Value = serde_json::from_str(&request_outcome.content)
            .context("Unable to parse listen key response for Binance")?;
        let listen_key = data["listenKey"]
            .as_str()
            .context("Unable to parse listen key field for Binance")?;

        let ws_path = format!("{}{}", "/ws/", listen_key);
        Ok(ws_path)
    }

    pub(crate) async fn request_open_orders_by_http_header(
        &self,
        http_params: Vec<(String, String)>,
    ) -> Result<RestRequestOutcome> {
        let url_path = match self.settings.is_margin_trading {
            true => "/fapi/v1/openOrders",
            false => "/api/v3/openOrders",
        };

        let full_url = rest_client::build_uri(&self.hosts.rest_host, url_path, &http_params)?;

        let orders = self.rest_client.get(full_url, &self.settings.api_key).await;

        orders
    }

    fn binance_position_to_active_position(
        &self,
        binance_position: BinancePosition,
    ) -> ActivePosition {
        let currency_pair = self
            .get_unified_currency_pair(&binance_position.specific_currency_pair)
            .with_expect(|| {
                format!(
                    "Failed to get_unified_currency_pair for {:?}",
                    binance_position.specific_currency_pair
                )
            });

        let side = match binance_position.position_side > dec!(0) {
            true => OrderSide::Buy,
            false => OrderSide::Sell,
        };

        let derivative_position = DerivativePosition::new(
            currency_pair,
            binance_position.position_amount,
            Some(side),
            dec!(0),
            binance_position.liquidation_price,
            binance_position.leverage,
        );

        ActivePosition::new(derivative_position)
    }
}

fn get_order_book_side(levels: &Vec<Value>) -> Result<SortedOrderData> {
    levels
        .iter()
        .map(|x| {
            let price = x[0]
                .as_str()
                .ok_or(anyhow!("Unable parse price of order book side in Binance"))?
                .parse()?;
            let amount = x[1]
                .as_str()
                .ok_or(anyhow!("Unable parse amount of order book side in Binance"))?
                .parse()?;
            Ok((price, amount))
        })
        .try_collect()
}
