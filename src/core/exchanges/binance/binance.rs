use super::support::BinanceOrderInfo;
use crate::core::exchanges::{
    common::CurrencyCode,
    general::features::{ExchangeFeatures, OpenOrdersType},
};
use crate::core::exchanges::{common::CurrencyId, general::exchange::BoxExchangeClient};
use crate::core::exchanges::{
    common::{CurrencyPair, ExchangeAccountId, RestRequestOutcome, SpecificCurrencyPair},
    events::AllowedEventSourceType,
};
use crate::core::exchanges::{
    general::handlers::handle_order_filled::FillEventData, rest_client, utils,
};
use crate::core::orders::fill::EventSourceType;
use crate::core::orders::order::*;
use crate::core::settings::ExchangeSettings;
use crate::core::{exchanges::traits::ExchangeClientBuilder, orders::fill::OrderFillType};
use anyhow::{anyhow, bail, Context, Result};
use dashmap::DashMap;
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
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>>,
    pub order_cancelled_callback:
        Mutex<Box<dyn FnMut(ClientOrderId, ExchangeOrderId, EventSourceType) + Send + Sync>>,
    pub handle_order_filled_callback: Mutex<Box<dyn FnMut(FillEventData) + Send + Sync>>,

    pub unified_to_specific: HashMap<CurrencyPair, SpecificCurrencyPair>,
    pub specific_to_unified: HashMap<SpecificCurrencyPair, CurrencyPair>,
    pub supported_currencies: DashMap<CurrencyId, CurrencyCode>,
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
            handle_order_filled_callback: Mutex::new(Box::new(|_| {})),
            unified_to_specific,
            specific_to_unified,
            supported_currencies: Default::default(),
        }
    }

    pub async fn get_listen_key(&self) -> Result<RestRequestOutcome> {
        let url_path = match self.settings.is_marging_trading {
            true => "/sapi/v1/userDataStream",
            false => "/api/v3/userDataStream",
        };

        let full_url = rest_client::build_uri(&self.settings.rest_host, url_path, &vec![])?;
        let http_params = rest_client::HttpParams::new();
        rest_client::send_post_request(full_url, &self.settings.api_key, &http_params).await
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

    fn generate_signature(&self, data: String) -> Result<String> {
        let mut hmac = Hmac::<Sha256>::new_varkey(self.settings.secret_key.as_bytes())
            .context("Unable to calculate hmac")?;
        hmac.update(data.as_bytes());
        let result = hex::encode(&hmac.finalize().into_bytes());

        return Ok(result);
    }

    pub(super) fn add_authentification_headers(
        &self,
        parameters: &mut rest_client::HttpParams,
    ) -> Result<()> {
        let time_stamp = utils::get_current_milliseconds();
        parameters.push(("timestamp".to_owned(), time_stamp.to_string()));

        let message_to_sign = rest_client::to_http_string(&parameters);
        let signature = self.generate_signature(message_to_sign)?;
        parameters.push(("signature".to_owned(), signature));

        Ok(())
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

    pub(super) fn handle_trade(&self, msg_to_log: &str, json_response: Value) -> Result<()> {
        let client_order_id = json_response["c"]
            .as_str()
            .ok_or(anyhow!("Unable to parse client order id"))?;
        let exchange_order_id = json_response["i"].to_string();
        let exchange_order_id = exchange_order_id.as_str();
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
                _ => error!(
                    "execution_type is NEW but order_status is {} for message {}",
                    order_status, msg_to_log
                ),
            },
            "CANCELED" => match order_status {
                "CANCELED" => {
                    let client_order_id = json_response["C"]
                        .as_str()
                        .ok_or(anyhow!("Unable to parse client order id"))?;
                    (&self.order_cancelled_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.into(),
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
                    let client_order_id = json_response["C"]
                        .as_str()
                        .ok_or(anyhow!("Uanble to parse client order id"))?;
                    (&self.order_cancelled_callback).lock()(
                        client_order_id.into(),
                        exchange_order_id.into(),
                        EventSourceType::WebSocket,
                    );
                }
                _ => error!(
                    "Order {} was expired, message: {}",
                    client_order_id, msg_to_log
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
            _ => error!("Impossible execution type"),
        }

        Ok(())
    }

    fn get_currency_code(&self, currency_id: &CurrencyId) -> Option<CurrencyCode> {
        self.supported_currencies
            .get(currency_id)
            .map(|some| some.value().clone())
    }

    fn prepare_data_for_fill_handler(
        &self,
        json_response: &Value,
        execution_type: &str,
        client_order_id: ClientOrderId,
        exchange_order_id: ExchangeOrderId,
    ) -> Result<FillEventData> {
        let trade_id = json_response["t"]
            .as_str()
            .ok_or(anyhow!("Unable to parse trade id"))?
            .to_owned();
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

        let fill_type = Self::get_fill_type(execution_type)?;
        let order_role = if is_maker {
            OrderRole::Maker
        } else {
            OrderRole::Taker
        };

        let event_data = FillEventData {
            source_type: EventSourceType::WebSocket,
            trade_id,
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
        };

        Ok(event_data)
    }

    // According to https://binance-docs.github.io/apidocs/futures/en/#event-order-update
    fn get_fill_type(raw_type: &str) -> Result<OrderFillType> {
        match raw_type {
            "CALCULATED" => Ok(OrderFillType::Liquidation),
            "FILL" => Ok(OrderFillType::UserTrade),
            "TRADE" => Ok(OrderFillType::UserTrade),
            "PARTIAL_FILL" => Ok(OrderFillType::UserTrade),
            _ => bail!("Unable to map trade type"),
        }
    }
}

pub struct BinanceBuilder;

impl ExchangeClientBuilder for BinanceBuilder {
    fn create_exchange_client(
        &self,
        exchange_settings: ExchangeSettings,
    ) -> (BoxExchangeClient, ExchangeFeatures) {
        let exchange_account_id = exchange_settings.exchange_account_id.clone();

        (
            Box::new(Binance::new(exchange_settings, exchange_account_id)) as BoxExchangeClient,
            ExchangeFeatures::new(
                OpenOrdersType::AllCurrencyPair,
                false,
                false,
                AllowedEventSourceType::All,
                AllowedEventSourceType::All,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_signature() {
        // All values and strings gotten from binane API example
        let right_value = "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71";

        let exchange_account_id: ExchangeAccountId = "Binance0".parse().expect("in test");

        let settings = ExchangeSettings::new(
            exchange_account_id.clone(),
            "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A".into(),
            "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j".into(),
            false,
        );

        let binance = Binance::new(settings, exchange_account_id);
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
