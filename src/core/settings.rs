use crate::core::exchanges::common::{CurrencyCode, CurrencyPair, ExchangeAccountId};
use serde::{Deserialize, Serialize};

use super::exchanges::common::Amount;

pub trait BaseStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId;
    fn currency_pair(&self) -> CurrencyPair;
    fn max_amount(&self) -> Amount;
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct AppSettings<TStrategySettings>
where
    TStrategySettings: BaseStrategySettings + Clone,
{
    pub strategy: TStrategySettings,
    pub core: CoreSettings,
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct CoreSettings {
    pub exchanges: Vec<ExchangeSettings>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CurrencyPairSetting {
    pub base: CurrencyCode,
    pub quote: CurrencyCode,
    // currency code specific for exchange
    pub currency_pair: Option<String>,
}

// Field order are matter for serialization:
// Simple values must be emmited before tables values as vectors
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExchangeSettings {
    // TODO add other settings
    pub api_key: String,
    pub secret_key: String,
    pub is_margin_trading: bool,
    // TODO change String to URI
    pub web_socket_host: String,
    // Some exchanges have two websockets, for public and private data
    pub web_socket2_host: String,
    pub rest_host: String,
    pub subscribe_to_market_data: bool,
    pub websocket_channels: Vec<String>,
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pairs: Option<Vec<CurrencyPairSetting>>,
}

impl ExchangeSettings {
    // only for tests
    pub fn new_short(
        exchange_account_id: ExchangeAccountId,
        api_key: String,
        secret_key: String,
        is_margin_trading: bool,
    ) -> Self {
        Self {
            exchange_account_id,
            api_key,
            secret_key,
            is_margin_trading,
            web_socket_host: "".into(),
            web_socket2_host: "".into(),
            rest_host: "".into(),
            websocket_channels: vec![],
            currency_pairs: None,
            subscribe_to_market_data: true,
        }
    }
}

impl Default for ExchangeSettings {
    fn default() -> Self {
        ExchangeSettings {
            exchange_account_id: ExchangeAccountId::new("".into(), 0),
            api_key: "".to_string(),
            secret_key: "".to_string(),
            is_margin_trading: false,
            web_socket_host: "".to_string(),
            web_socket2_host: "".to_string(),
            rest_host: "".to_string(),
            websocket_channels: vec![],
            currency_pairs: None,
            subscribe_to_market_data: true,
        }
    }
}
