use crate::core::settings::ExchangeSettings;
use crate::core::exchanges::common::{RestRequestResult, RestErrorDescription, ExchangeErrorType, CurrencyPair};
use serde_json::Value;
use itertools::Itertools;

pub struct Binance {
    pub id: String
}

impl Binance {
    pub fn extend_settings(settings: &mut ExchangeSettings){
        if settings.is_marging_trading {
            settings.web_socket_host = "wss://fstream.binance.com".to_string();
            settings.web_socket2_host = "wss://fstream3.binance.com".to_string();
            settings.rest_host = "https://fapi.binance.com".to_string();
        }
        else {
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
                    return Some(RestErrorDescription::new(data["msg"].to_string(), data["code"].as_u64().unwrap() as u32));
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
            "Unknown order sent."
            | "Order does not exist." => ExchangeErrorType::OrderNotFound,
            "Account has insufficient balance for requested action." => ExchangeErrorType::InsufficientFunds,
            "Invalid quantity."
            | "Filter failure: MIN_NOTIONAL"
            | "Filter failure: LOT_SIZE"
            | "Filter failure: PRICE_FILTER"
            | "Filter failure: PERCENT_PRICE"
            | "Quantity less than zero."
            | "Precision is over the maximum defined for this asset." => ExchangeErrorType::InvalidOrder,
            msg if msg.contains("Too many requests;") => ExchangeErrorType::RateLimit,
            _ => ExchangeErrorType::Unknown
        }
    }

    pub async fn reconnect(&mut self) {
        todo!("reconnect")
    }

    pub fn build_ws1_path(currency_pairs: &[CurrencyPair], websocket_channels: &[String]) -> String {
        let stream_names = currency_pairs.iter().flat_map(|currency_pair| {
            //websocket_channels.iter().map(|channel| format!("{}@{}", currency_pair.as_str(), channel))
            let mut results = Vec::new();
            for channel in websocket_channels {
                let result = Self::get_stream_name(currency_pair, channel);
                results.push(result);
            }
            results
        }).join("/");
        let ws_path = format!("/stream?streams={}", stream_names);
        ws_path.to_lowercase()
    }

    fn get_stream_name(currency_pair: &CurrencyPair, channel: &str) -> String { format!("{}@{}", currency_pair.as_str(), channel) }

    fn is_websocket_reconnecting(&self) -> bool {
        todo!("is_websocket_reconnecting")
    }
}

