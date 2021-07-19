use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use awc::http::Uri;
use chrono::Utc;
use dashmap::DashMap;
use itertools::Itertools;
use log::{error, info};
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::binance::Binance;
use crate::core::exchanges::common::SortedOrderData;
use crate::core::exchanges::events::ExchangeEvent;
use crate::core::exchanges::{
    common::CurrencyCode, common::CurrencyId,
    general::currency_pair_metadata::CurrencyPairMetadata,
    general::handlers::handle_order_filled::FillEventData, traits::Support,
};
use crate::core::order_book::event::{EventType, OrderBookEvent};
use crate::core::order_book::order_book_data::OrderBookData;
use crate::core::orders::order::*;
use crate::core::{
    connectivity::connectivity_manager::WebSocketRole,
    exchanges::general::currency_pair_metadata::Precision,
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

        match serde_json::from_str::<Value>(&response.content) {
            Ok(data) => {
                let message = data["msg"].as_str();
                let code = data["code"].as_i64();

                match message {
                    None => Err(ExchangeError::new(
                        ExchangeErrorType::ParsingError,
                        "Unable to parse msg field".into(),
                        None,
                    )),
                    Some(message) => match code {
                        None => Err(ExchangeError::new(
                            ExchangeErrorType::ParsingError,
                            "Unable to parse code field".into(),
                            None,
                        )),
                        Some(code) => Err(ExchangeError::new(
                            ExchangeErrorType::Unknown,
                            message.to_string(),
                            Some(code),
                        )),
                    },
                }
            }
            Err(error) => {
                let error_message = format!("Unable to parse response.content: {}", error);
                Err(ExchangeError::new(
                    ExchangeErrorType::ParsingError,
                    error_message,
                    None,
                ))
            }
        }
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
        let data: Value = serde_json::from_str(msg).context("Unable to parse websocket message")?;
        // Public stream
        if let Some(stream) = data.get("stream") {
            let stream = stream
                .as_str()
                .ok_or(anyhow!("Unable to parse stream data"))?;

            if let Some(byte_index) = stream.find('@') {
                let currency_pair = self.currency_pair_from_web_socket(&stream[..byte_index])?;
                let data = &data["data"];

                // TODO handle public stream
                if stream.ends_with("depth20") {
                    self.process_snapshot_update(&currency_pair, data)?;
                }
            }

            return Ok(());
        }

        // so it is userData stream
        let event_type = data["e"]
            .as_str()
            .ok_or(anyhow!("Unable to parse event_type"))?;
        if event_type == "executionReport" {
            self.handle_trade(msg, data)?;
        } else if false {
            // TODO something about ORDER_TRADE_UPDATE? There are no info about it in Binance docs
        } else {
            self.log_unknown_message(self.id.clone(), msg);
        }

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

    fn is_enabled_websocket(&self, role: WebSocketRole) -> bool {
        match role {
            WebSocketRole::Main => true,
            WebSocketRole::Secondary => {
                self.settings.api_key != "" && self.settings.secret_key == ""
            }
        }
    }

    async fn create_ws_url(&self, role: WebSocketRole) -> Result<Uri> {
        let (host, path) = match role {
            WebSocketRole::Main => (
                &self.settings.web_socket_host,
                self.build_ws_main_path(&self.settings.websocket_channels[..]),
            ),
            WebSocketRole::Secondary => (
                &self.settings.web_socket2_host,
                self.build_ws_secondary_path().await?,
            ),
        };

        format!("{}{}", host, path)
            .parse::<Uri>()
            .with_context(|| format!("Unable parse websocket {:?} uri", role))
    }

    fn get_specific_currency_pair(&self, currency_pair: &CurrencyPair) -> SpecificCurrencyPair {
        self.unified_to_specific[currency_pair].clone()
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
        info!("Unknown message for {}: {}", exchange_account_id, message);
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

    fn parse_metadata(
        &self,
        _response: &RestRequestOutcome,
    ) -> Result<Vec<Arc<CurrencyPairMetadata>>> {
        // TODO parse metadata
        // This is just a stub
        Ok(vec![
            Arc::new(CurrencyPairMetadata {
                base_currency_id: "PHB".into(),
                base_currency_code: "phb".into(),
                quote_currency_id: "BTC".into(),
                quote_currency_code: "btc".into(),
                is_active: true,
                is_derivative: true,
                min_price: Some(dec!(0.00000001)),
                max_price: Some(dec!(1000)),
                amount_currency_code: "phb".into(),
                min_amount: Some(dec!(1)),
                max_amount: Some(dec!(90000000)),
                min_cost: Some(dec!(0.0001)),
                balance_currency_code: Some("phb".into()),
                price_precision: Precision::ByFraction { precision: 8 },
                amount_precision: Precision::ByFraction { precision: 0 },
            }),
            Arc::new(CurrencyPairMetadata {
                base_currency_id: "ETH".into(),
                base_currency_code: "eth".into(),
                quote_currency_id: "BTC".into(),
                quote_currency_code: "btc".into(),
                is_active: true,
                is_derivative: true,
                min_price: Some(dec!(0.000001)),
                max_price: Some(dec!(922327)),
                amount_currency_code: "eth".into(),
                min_amount: Some(dec!(0.001)),
                max_amount: Some(dec!(100000)),
                min_cost: Some(dec!(0.0001)),
                balance_currency_code: Some("eth".into()),
                price_precision: Precision::ByFraction { precision: 6 },
                amount_precision: Precision::ByFraction { precision: 3 },
            }),
            Arc::new(CurrencyPairMetadata {
                base_currency_id: "EOS".into(),
                base_currency_code: "eos".into(),
                quote_currency_id: "BTC".into(),
                quote_currency_code: "btc".into(),
                is_active: true,
                is_derivative: true,
                min_price: Some(dec!(0.0000001)),
                max_price: Some(dec!(1000)),
                amount_currency_code: "eos".into(),
                min_amount: Some(dec!(0.01)),
                max_amount: Some(dec!(90000000)),
                min_cost: Some(dec!(0.0001)),
                balance_currency_code: Some("eos".into()),
                price_precision: Precision::ByFraction { precision: 7 },
                amount_precision: Precision::ByFraction { precision: 2 },
            }),
        ])
    }
}

impl Binance {
    pub fn process_snapshot_update(
        &self,
        currency_pair: &CurrencyPair,
        data: &Value,
    ) -> Result<()> {
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
        currency_pair: &CurrencyPair,
        event_id: &str,
        order_book_data: OrderBookData,
        order_book_update: Option<Vec<OrderBookData>>,
    ) -> Result<()> {
        if !self.subscribe_to_market_data {
            return Ok(());
        }

        let mut order_book_event = OrderBookEvent::new(
            Utc::now(),
            self.id.clone(),
            currency_pair.clone(),
            event_id.to_string(),
            EventType::Snapshot,
            order_book_data,
        );

        //Some exchanges like Binance don't give us Snapshot in Web Socket, so we have to request Snapshot using Rest
        //and then update it with orderBookUpdates that we received while Rest request was being executed
        if let Some(updates) = order_book_update {
            order_book_event.apply_data_update(updates)
        }

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
                error!("{}", msg);
                self.application_manager
                    .clone()
                    .spawn_graceful_shutdown(msg.clone());
                Err(anyhow!(msg))
            }
        }
    }

    fn build_ws_main_path(&self, websocket_channels: &[String]) -> String {
        let stream_names = self
            .specific_to_unified
            .keys()
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
