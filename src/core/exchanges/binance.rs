use super::common_interaction::CommonInteraction;
use super::rest_client;
use super::utils;
use crate::core::exchanges::common::{
    Amount, CurrencyPair, ExchangeAccountId, ExchangeErrorType, Price, RestErrorDescription,
    RestRequestOutcome, SpecificCurrencyPair,
};
use crate::core::orders::fill::EventSourceType;
use crate::core::orders::order::*;
use crate::core::settings::ExchangeSettings;
use async_trait::async_trait;
use hex;
use hmac::{Hmac, Mac, NewMac};
use itertools::Itertools;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Binance {
    pub settings: ExchangeSettings,
    pub id: ExchangeAccountId,
    pub cb_websocket_msg_received:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>>,

    pub unified_to_specific: HashMap<CurrencyPair, SpecificCurrencyPair>,
    pub specific_to_unified: HashMap<SpecificCurrencyPair, CurrencyPair>,
}

impl Binance {
    pub fn new(settings: ExchangeSettings, id: ExchangeAccountId) -> Self {
        let unified_phbbtc = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let specific_phbbtc = SpecificCurrencyPair::new("PHBBTC".into());

        let mut unified_to_specific = HashMap::new();
        unified_to_specific.insert(unified_phbbtc.clone(), specific_phbbtc.clone());

        let mut specific_to_unified = HashMap::new();
        specific_to_unified.insert(specific_phbbtc, unified_phbbtc);

        Self {
            settings,
            id,
            cb_websocket_msg_received: Mutex::new(Box::new(|_, _, _| {})),
            unified_to_specific,
            specific_to_unified,
        }
    }

    pub fn set_cb_websocket_msg_received(
        &self,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>,
    ) {
        *self.cb_websocket_msg_received.lock() = callback;
    }

    pub fn extend_settings(settings: &mut ExchangeSettings) {
        if settings.is_marging_trading {
            settings.web_socket_host = "wss://fstream.binance.com".to_string();
            settings.web_socket2_host = "wss://fstream3.binance.com".to_string();
            settings.rest_host = "https://fapi.binance.com".to_string();
        } else {
            settings.web_socket_host = "wss://stream.binance.com:9443".to_string();
            settings.web_socket2_host = "wss://stream.binance.com:9443".to_string();
            settings.rest_host = "https://api.binance.com".to_string();
        }
    }

    pub async fn reconnect(&mut self) {
        todo!("reconnect")
    }

    pub fn build_ws1_path(
        specific_currency_pairs: &[SpecificCurrencyPair],
        websocket_channels: &[String],
    ) -> String {
        let stream_names = specific_currency_pairs
            .iter()
            .flat_map(|currency_pair| {
                //websocket_channels.iter().map(|channel| format!("{}@{}", currency_pair.as_str(), channel))
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

    fn get_stream_name(specific_currency_pair: &SpecificCurrencyPair, channel: &str) -> String {
        format!("{}@{}", specific_currency_pair.as_str(), channel)
    }

    fn is_websocket_reconnecting(&self) -> bool {
        todo!("is_websocket_reconnecting")
    }

    fn to_server_order_side(side: OrderSide) -> String {
        match side {
            OrderSide::Buy => "BUY".to_owned(),
            OrderSide::Sell => "SELL".to_owned(),
        }
    }

    fn to_local_order_side(side: &str) -> OrderSide {
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

    fn to_server_order_type(order_type: OrderType) -> String {
        match order_type {
            OrderType::Limit => "LIMIT".to_owned(),
            OrderType::Market => "MARKET".to_owned(),
            unexpected_variant => panic!("{:?} are not expected", unexpected_variant),
        }
    }

    fn generate_signature(&self, data: String) -> String {
        // TODO fix that unwrap But dunno how
        let mut hmac = Hmac::<Sha256>::new_varkey(self.settings.secret_key.as_bytes()).unwrap();
        hmac.update(data.as_bytes());
        let result = hex::encode(&hmac.finalize().into_bytes());

        return result;
    }

    fn add_authentification_headers(&self, parameters: &mut rest_client::HttpParams) {
        let time_stamp = utils::get_current_milliseconds();
        parameters.push(("timestamp".to_owned(), time_stamp.to_string()));

        let message_to_sign = rest_client::to_http_string(&parameters);
        let signature = self.generate_signature(message_to_sign);
        parameters.push(("signature".to_owned(), signature));
    }

    pub fn get_unified_currency_pair(&self, currency_pair: &SpecificCurrencyPair) -> CurrencyPair {
        self.specific_to_unified[&currency_pair].clone()
    }

    fn specific_order_info_to_unified(&self, specific: &BinanceOrderInfo) -> OrderInfo {
        OrderInfo::new(
            self.get_unified_currency_pair(&specific.specific_currency_pair),
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

    fn handle_trade(&self, msg_to_log: &str, json_response: &str) {
        // TODO parse json_response
        (&self.cb_websocket_msg_received).lock()(
            "some client_order_id".into(),
            "some exchange_order_id".into(),
            EventSourceType::WebSocket,
        );
    }
}

#[async_trait(?Send)]
impl CommonInteraction for Binance {
    async fn create_order(&self, order: &OrderCreating) -> RestRequestOutcome {
        let specific_currency_pair = self.get_specific_currency_pair(&order.header.currency_pair);

        let mut parameters = vec![
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
            parameters.push(("timeInForce".to_owned(), "GTC".to_owned()));
            parameters.push(("price".to_owned(), order.price.to_string()));
        }

        if order.header.execution_type == OrderExecutionType::MakerOnly {
            parameters.push(("timeInForce".to_owned(), "GTX".to_owned()));
        }

        // TODO What is marging trading?
        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/order"
        } else {
            "/api/v3/order"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        self.add_authentification_headers(&mut parameters);

        rest_client::send_post_request(&full_url, &self.settings.api_key, &parameters).await
    }

    fn get_specific_currency_pair(&self, currency_pair: &CurrencyPair) -> SpecificCurrencyPair {
        self.unified_to_specific[currency_pair].clone()
    }

    fn on_websocket_message(&self, msg: String) {
        dbg!(&msg);
        // TODO a lot of other stuff
        // FIXME decerealize data
        let json_req = "some json";
        self.handle_trade(&msg, json_req);
    }

    fn is_rest_error_code(&self, response: &RestRequestOutcome) -> Option<RestErrorDescription> {
        //Binance is a little inconsistent: for failed responses sometimes they include
        //only code or only success:false but sometimes both
        if response.content.contains(r#""success":false"#) || response.content.contains(r#""code""#)
        {
            let data: Value = serde_json::from_str(&response.content).unwrap();
            return Some(RestErrorDescription::new(
                data["msg"].as_str().unwrap().to_owned(),
                data["code"].as_i64().unwrap() as i64,
            ));
        }

        None
    }

    fn get_order_id(&self, response: &RestRequestOutcome) -> ExchangeOrderId {
        let response: Value = serde_json::from_str(&response.content).unwrap();
        let id = response["orderId"].to_string();
        ExchangeOrderId::new(id.into())
    }

    fn get_error_type(&self, error: &RestErrorDescription) -> ExchangeErrorType {
        // -1010 ERROR_MSG_RECEIVED
        // -2010 NEW_ORDER_REJECTED
        // -2011 CANCEL_REJECTED
        match error.message.as_str() {
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
        }
    }

    // TODO not implemented correctly
    async fn get_account_info(&self) {
        let mut parameters = rest_client::HttpParams::new();

        self.add_authentification_headers(&mut parameters);

        let path_to_get_account_data = "/api/v3/account";
        let full_url = format! {"{}{}", self.settings.rest_host, path_to_get_account_data};

        rest_client::send_get_request(&full_url, &self.settings.api_key, &parameters).await;
    }

    // TODO not implemented correctly
    async fn cancel_order(&self, order: &OrderCancelling) -> RestRequestOutcome {
        let specific_currency_pair = self.get_specific_currency_pair(&order.currency_pair);
        let mut parameters = rest_client::HttpParams::new();
        parameters.push((
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        ));
        parameters.push(("orderId".to_owned(), order.order_id.as_str().to_owned()));

        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/order"
        } else {
            "/api/v3/order"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        self.add_authentification_headers(&mut parameters);

        let outcome =
            rest_client::send_delete_request(&full_url, &self.settings.api_key, &parameters).await;

        outcome
    }

    async fn get_open_orders(&self) -> RestRequestOutcome {
        let mut parameters = rest_client::HttpParams::new();
        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/openOrders"
        } else {
            "/api/v3/openOrders"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        self.add_authentification_headers(&mut parameters);
        let orders =
            rest_client::send_get_request(&full_url, &self.settings.api_key, &parameters).await;

        orders
    }

    fn parse_open_orders(&self, response: &RestRequestOutcome) -> Vec<OrderInfo> {
        // TODO that unwrap has to be just logging
        let binance_orders: Vec<BinanceOrderInfo> =
            serde_json::from_str(&response.content).unwrap();

        let orders_info: Vec<OrderInfo> = binance_orders
            .iter()
            .map(|order| self.specific_order_info_to_unified(order))
            .collect();

        orders_info
    }

    // TODO not implemented correctly
    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) {
        let specific_currency_pair = self.get_specific_currency_pair(&currency_pair);
        let path_to_delete = "/api/v3/openOrders";
        let mut full_url = self.settings.rest_host.clone();
        full_url.push_str(path_to_delete);

        let mut parameters = rest_client::HttpParams::new();
        parameters.push((
            "symbol".to_owned(),
            specific_currency_pair.as_str().to_owned(),
        ));

        self.add_authentification_headers(&mut parameters);

        let _cancel_order_outcome =
            rest_client::send_delete_request(&full_url, &self.settings.api_key, &parameters).await;
    }

    fn set_websocket_msg_received(
        self: Arc<Self>,
        callback: Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>,
    ) {
        *self.cb_websocket_msg_received.lock() = callback;
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_signature() {
        // All values and strings gotten from binane API example
        let right_value = "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71";

        let settings = ExchangeSettings {
            api_key: "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A".into(),
            secret_key: "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j".into(),
            is_marging_trading: false,
            web_socket_host: "".into(),
            web_socket2_host: "".into(),
            rest_host: "https://api.binance.com".into(),
        };

        let binance = Binance::new(settings, "Binance0".parse().unwrap());
        let params = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559".into();
        let result = binance.generate_signature(params);
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
