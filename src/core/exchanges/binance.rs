use super::common::CurrencyPair;
use super::common_interaction::CommonInteraction;
use super::rest_client;
use crate::core::exchanges::common::{
    ExchangeErrorType, RestErrorDescription, RestRequestOutcome, SpecificCurrencyPair,
};
use crate::core::orders::order::{
    OrderCancelling, OrderCreating, OrderExecutionType, OrderSide, OrderType,
}; //TODO first word in each type can be replaced just using module name
use crate::core::settings::ExchangeSettings;
use async_trait::async_trait;
use hex;
use hmac::{Hmac, Mac, NewMac};
use itertools::Itertools;
use serde_json::Value;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Clone)]
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

    // TODO Optional here, really? It means we have HTTP error code, but Binance says everything is great
    pub fn get_error_description(binance_error_msg: &str) -> Option<RestErrorDescription> {
        //Binance is a little inconsistent: for failed responses sometimes they include
        //only code or only success:false but sometimes both
        if binance_error_msg.contains(r#""success":false"#)
            || binance_error_msg.contains(r#""code""#)
        {
            let data: Value = serde_json::from_str(binance_error_msg).unwrap();
            return Some(RestErrorDescription::new(
                data["msg"].to_string(),
                data["code"].as_i64().unwrap() as i64,
            ));
        } else {
            None
        }
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
            _ => panic!("Other options are not expected"),
        }
    }

    fn generate_signature(&self, data: String) -> String {
        // TODO fix that unwrap But dunno how
        let mut hmac = Hmac::<Sha256>::new_varkey(self.settings.secret_key.as_bytes()).unwrap();
        hmac.update(data.as_bytes());
        let result = hex::encode(&hmac.finalize().into_bytes());

        return result;
    }

    fn add_autentification_headers(
        &self,
        mut parameters: rest_client::HttpParams,
    ) -> rest_client::HttpParams {
        // TODO extract to utils?
        let time_stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        parameters.push(("timestamp".to_owned(), time_stamp.to_string()));

        let message_to_sign = Self::to_http_string(&parameters);
        let signature = self.generate_signature(message_to_sign);
        parameters.push(("signature".to_owned(), signature));

        parameters
    }

    // TODO excract to utils?
    fn to_http_string(parameters: &rest_client::HttpParams) -> String {
        let mut http_string = String::new();
        for (key, value) in parameters.into_iter() {
            if !http_string.is_empty() {
                http_string.push('&');
            }
            http_string.push_str(&key);
            http_string.push('=');
            http_string.push_str(&value);
        }

        http_string
    }
}

#[async_trait(?Send)]
impl CommonInteraction for Binance {
    async fn create_order(&self, order: &OrderCreating) -> RestRequestOutcome {
        let mut parameters = vec![
            (
                "symbol".to_owned(),
                order.currency_pair.as_str().to_uppercase(),
            ),
            ("side".to_owned(), Self::to_server_order_side(order.side)),
            (
                "type".to_owned(),
                Self::to_server_order_type(order.order_type),
            ),
            ("quantity".to_owned(), order.amount.to_string()),
            (
                "newClientOrderId".to_owned(),
                order.client_order_id.as_str().to_owned(),
            ),
        ];

        if order.order_type != OrderType::Market {
            parameters.push(("timeInForce".to_owned(), "GTC".to_owned()));
            parameters.push(("price".to_owned(), order.price.to_string()));
        }

        if order.execution_type == OrderExecutionType::MakerOnly {
            parameters.push(("timeInForce".to_owned(), "GTX".to_owned()));
        }

        // TODO What is marging trading?
        let url_path = if self.settings.is_marging_trading {
            "/fapi/v1/order"
        } else {
            "/api/v3/order"
        };
        let full_url = format!("{}{}", self.settings.rest_host, url_path);

        let full_parameters = self.add_autentification_headers(parameters);

        rest_client::send_post_request(&full_url, &self.settings.api_key, full_parameters).await
    }

    // TODO not implemented correctly
    async fn get_account_info(&self) {
        let parameters = rest_client::HttpParams::new();

        let full_parameters = self.add_autentification_headers(parameters);

        let path_to_get_account_data = "/api/v3/account";
        let mut full_url = self.settings.rest_host.clone();
        full_url.push_str(path_to_get_account_data);

        rest_client::send_get_request(&full_url, &self.settings.api_key, full_parameters).await;
    }

    // TODO not implemented correctly
    async fn cancel_order(&self, order: &OrderCancelling) -> RestRequestOutcome {
        let mut parameters = rest_client::HttpParams::new();
        parameters.push((
            "symbol".to_owned(),
            order.currency_pair.as_str().to_uppercase(),
        ));
        parameters.push(("orderId".to_owned(), order.order_id.as_str().to_owned()));

        let mut full_url = self.settings.rest_host.clone();
        if self.settings.is_marging_trading {
            full_url.push_str("/fapi/v1/order");
        } else {
            full_url.push_str("/api/v3/order");
        }

        let full_parameters = self.add_autentification_headers(parameters);

        let outcome =
            rest_client::send_delete_request(&full_url, &self.settings.api_key, full_parameters)
                .await;

        dbg!(&outcome);

        outcome
    }

    // TODO not implemented correctly
    async fn cancel_all_orders(&self, currency_pair: CurrencyPair) {
        let path_to_delete = "/api/v3/openOrders";
        let mut full_url = self.settings.rest_host.clone();
        full_url.push_str(path_to_delete);

        let mut parameters = rest_client::HttpParams::new();
        parameters.push(("symbol".to_owned(), currency_pair.as_str().to_owned()));

        let full_parameters = self.add_autentification_headers(parameters);

        let cancel_order_outcome =
            rest_client::send_delete_request(&full_url, &self.settings.api_key, full_parameters)
                .await;

        dbg!(&cancel_order_outcome);
    }
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

        let binance = Binance::new(settings, "some_id".into());
        let params = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559".into();
        let result = binance.generate_signature(params);
        assert_eq!(result, right_value);
    }

    #[test]
    fn to_http_string() {
        let parameters = vec![
            ("symbol".to_owned(), "LTCBTC".to_owned()),
            ("side".to_owned(), "BUY".to_owned()),
            ("type".to_owned(), "LIMIT".to_owned()),
            ("timeInForce".to_owned(), "GTC".to_owned()),
            ("quantity".to_owned(), "1".to_owned()),
            ("price".to_owned(), "0.1".to_owned()),
            ("recvWindow".to_owned(), "5000".to_owned()),
            ("timestamp".to_owned(), "1499827319559".to_owned()),
        ];

        let http_string = Binance::to_http_string(&parameters);

        let right_value = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559";
        assert_eq!(http_string, right_value);
    }
}
