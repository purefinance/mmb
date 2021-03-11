use super::support::BinanceOrderInfo;
use crate::core::exchanges::common::{
    CurrencyPair, ExchangeAccountId, RestRequestOutcome, SpecificCurrencyPair,
};
use crate::core::exchanges::rest_client;
use crate::core::exchanges::utils;
use crate::core::orders::fill::EventSourceType;
use crate::core::orders::order::*;
use crate::core::settings::ExchangeSettings;
use hex;
use hmac::{Hmac, Mac, NewMac};
use log::error;
use parking_lot::Mutex;
use serde_json::Value;
use sha2::Sha256;
use std::collections::HashMap;

pub struct Binance {
    pub settings: ExchangeSettings,
    pub id: ExchangeAccountId,
    pub order_created_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>>,
    pub order_cancelled_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType)>>,

    pub unified_to_specific: HashMap<CurrencyPair, SpecificCurrencyPair>,
    pub specific_to_unified: HashMap<SpecificCurrencyPair, CurrencyPair>,
}

impl Binance {
    pub fn new(mut settings: ExchangeSettings, id: ExchangeAccountId) -> Self {
        let unified_phbbtc = CurrencyPair::from_currency_codes("phb".into(), "btc".into());
        let specific_phbbtc = SpecificCurrencyPair::new("PHBBTC".into());

        let mut unified_to_specific = HashMap::new();
        unified_to_specific.insert(unified_phbbtc.clone(), specific_phbbtc.clone());

        let mut specific_to_unified = HashMap::new();
        specific_to_unified.insert(specific_phbbtc, unified_phbbtc);

        Self::extend_settings(&mut settings);

        Self {
            settings,
            id,
            order_created_callback: Mutex::new(Box::new(|_, _, _| {})),
            order_cancelled_callback: Mutex::new(Box::new(|_, _, _| {})),
            unified_to_specific,
            specific_to_unified,
        }
    }

    pub async fn get_listen_key(&self) -> RestRequestOutcome {
        let url_path = if self.settings.is_marging_trading {
            "/sapi/v1/userDataStream"
        } else {
            "/api/v3/userDataStream"
        };

        let full_url = format!("{}{}", self.settings.rest_host, url_path);
        let parameters = rest_client::HttpParams::new();
        rest_client::send_post_request(&full_url, &self.settings.api_key, &parameters).await
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

    pub(super) fn get_stream_name(
        specific_currency_pair: &SpecificCurrencyPair,
        channel: &str,
    ) -> String {
        format!("{}@{}", specific_currency_pair.as_str(), channel)
    }

    fn is_websocket_reconnecting(&self) -> bool {
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

    fn generate_signature(&self, data: String) -> String {
        // TODO fix that unwrap But dunno how
        let mut hmac = Hmac::<Sha256>::new_varkey(self.settings.secret_key.as_bytes()).unwrap();
        hmac.update(data.as_bytes());
        let result = hex::encode(&hmac.finalize().into_bytes());

        return result;
    }

    pub(super) fn add_authentification_headers(&self, parameters: &mut rest_client::HttpParams) {
        let time_stamp = utils::get_current_milliseconds();
        parameters.push(("timestamp".to_owned(), time_stamp.to_string()));

        let message_to_sign = rest_client::to_http_string(&parameters);
        let signature = self.generate_signature(message_to_sign);
        parameters.push(("signature".to_owned(), signature));
    }

    pub fn get_unified_currency_pair(&self, currency_pair: &SpecificCurrencyPair) -> CurrencyPair {
        self.specific_to_unified[&currency_pair].clone()
    }

    pub(super) fn specific_order_info_to_unified(&self, specific: &BinanceOrderInfo) -> OrderInfo {
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

    pub(super) fn handle_trade(&self, msg_to_log: &str, json_response: Value) {
        let client_order_id = json_response["c"].as_str().unwrap();
        let exchange_order_id = json_response["i"].to_string();
        let execution_type = json_response["x"].as_str().unwrap();
        let order_status = json_response["X"].as_str().unwrap();
        let time_in_force = json_response["f"].as_str().unwrap();

        match execution_type {
            "NEW" => match order_status {
                "NEW" => {
                    (&self.order_created_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.as_str().into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => error!(
                    "execution_type is NEW but order_status is {} for message {}",
                    order_status, msg_to_log
                ),
            },
            "CANCELED" => match order_status {
                "CANCELED" => {
                    let client_order_id = json_response["C"].as_str().unwrap();
                    (&self.order_cancelled_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.as_str().into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => error!(
                    "execution_type is CANCELED but order_status is {} for message {}",
                    order_status, msg_to_log
                ),
            },
            "REJECTED" => {
                // TODO: May be not handle error in Rest but move it here to make it unified?
                // We get notification of rejected orders from the rest responses
            }
            "EXPIRED" => match time_in_force {
                "GTX" => {
                    let client_order_id = json_response["C"].as_str().unwrap();
                    (&self.order_cancelled_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.as_str().into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => error!(
                    "Order {} was expired, message: {}",
                    client_order_id, msg_to_log
                ),
            },
            "TRADE" | "CALCULATED" => {} // TODO handle it,
            _ => error!("Impossible execution type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_signature() {
        // All values and strings gotten from binane API example
        let right_value = "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71";

        let settings = ExchangeSettings::new(
            "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A".into(),
            "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j".into(),
            false,
        );

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
