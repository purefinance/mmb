use super::common_interaction::CommonInteraction;
use crate::core::exchanges::common::{
    ExchangeErrorType, RestErrorDescription, RestRequestResult, SpecificCurrencyPair,
};
use crate::core::orders::order::{OrderSide, OrderSnapshot, OrderType};
use crate::core::settings::ExchangeSettings;
use actix::{Actor, Context, Handler, Message, System};
use async_trait::async_trait;
use hex;
use hmac::{Hmac, Mac, NewMac};
use itertools::Itertools;
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;

pub struct Binance {
    pub settings: ExchangeSettings,
    pub id: String,
}

impl Binance {
    pub fn new(settings: ExchangeSettings, id: String) -> Self {
        Self { settings, id }
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

    fn is_rest_error_code(response: &RestRequestResult) -> Option<RestErrorDescription> {
        //Binance is a little inconsistent: for failed responses sometimes they include
        //only code or only success:false but sometimes both
        match response {
            Ok(content) => {
                if content.contains(r#""success":false"#) || content.contains(r#""code""#) {
                    let data: Value = serde_json::from_str(content).unwrap();
                    return Some(RestErrorDescription::new(
                        data["msg"].to_string(),
                        data["code"].as_u64().unwrap() as u32,
                    ));
                }
            }
            _ => (),
        };

        None
    }

    fn get_error_type(message: &str, _code: Option<u32>) -> ExchangeErrorType {
        // -1010 ERROR_MSG_RECEIVED
        // -2010 NEW_ORDER_REJECTED
        // -2011 CANCEL_REJECTED
        match message {
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

    fn get_hmac(&self, data: String) -> String {
        // TODO fix that unwrap But dunno how
        let mut hmac = Hmac::<Sha256>::new_varkey(self.settings.secret_key.as_bytes()).unwrap();
        hmac.update(data.as_bytes());
        let result = hex::encode(&hmac.finalize().into_bytes());

        return result;
    }

    fn to_server_order_side(side: OrderSide) -> String {
        match side {
            OrderSide::Buy => "BUY".to_owned(),
            OrderSide::Sell => "SELL".to_owned(),
        }
    }

    fn to_server_order_type(order_type: OrderType) -> String {
        match order_type {
            OrderType::Limit => "LIMIT".to_owned(),
            OrderType::Market => "MARKET".to_owned(),
            // TODO How to handle over types?
            _ => String::new(),
        }
    }
}

#[async_trait(?Send)]
impl CommonInteraction for Binance {
    async fn create_order(&self, order: &OrderSnapshot) {
        // TODO Handle it correctly
        if order.header.side.is_none() {
            dbg!(&"Unable to create order");
            return;
        }

        let order_side = Self::to_server_order_side(order.header.side.unwrap());
        let order_type = Self::to_server_order_type(order.header.order_type);

        let request_data = HashMap.new();
        request_data.insert("symbol", order.header.currency_pair.as_str().to_uppercase());
        //"side": order_side,
        //"type": order_type,
        //"quantity": order.header.amount.to_string(),

        if order.header.order_type != OrderType::Market {
            request_data.insert("test", "tst");
        }

        let client = awc::Client::default();
        let response = client
            .post("https://api.binance.com/api/v3/order/test")
            .header("X-MBX-APIKEY", self.settings.api_key.clone())
            .send()
            .await;
        dbg!(&response.unwrap().body().await);

        System::current().stop();
    }

    async fn cancel_order(&self) {
        dbg!(&"Cancel order for Binance!");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_hmac() {
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

        let binance = Binance::new(settings, "some_id".into());
        let params = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559".into();
        let result = binance.get_hmac(params);
        assert_eq!(result, right_value);
    }
}
