use crate::core::exchanges::common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId};
use serde::{Deserialize, Serialize};

pub trait BaseStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId;
    fn currency_pair(&self) -> CurrencyPair;
    fn max_amount(&self) -> Amount;
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct AppSettings<StrategySettings>
where
    StrategySettings: BaseStrategySettings + Clone,
{
    pub strategy: StrategySettings,
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
// Simple values must be emmited before struct with custom serialization
// https://github.com/alexcrichton/toml-rs/issues/142#issuecomment-278970591
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExchangeSettings {
    // TODO add other settings
    pub exchange_account_id: ExchangeAccountId,
    pub api_key: String,
    pub secret_key: String,
    pub is_margin_trading: bool,
    pub request_trades: bool,
    pub is_reducing_market_data: Option<bool>,
    pub subscribe_to_market_data: bool,
    pub websocket_channels: Vec<String>,
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
            request_trades: false,
            websocket_channels: vec![],
            currency_pairs: None,
            subscribe_to_market_data: true,
            is_reducing_market_data: None,
        }
    }
}

pub struct Hosts {
    pub web_socket_host: String,
    // Some exchanges have two websockets, for public and private data
    pub web_socket2_host: String,
    pub rest_host: String,
}

impl Default for ExchangeSettings {
    fn default() -> Self {
        ExchangeSettings {
            exchange_account_id: ExchangeAccountId::new("".into(), 0),
            api_key: "".to_string(),
            secret_key: "".to_string(),
            is_margin_trading: false,
            request_trades: false,
            websocket_channels: vec![],
            currency_pairs: None,
            subscribe_to_market_data: true,
            is_reducing_market_data: None,
        }
    }
}

pub struct CurrencyPriceSourceSettings {
    pub start_currency_code: CurrencyCode,
    pub end_currency_code: CurrencyCode,
    /// List of pairs ExchangeId and CurrencyPairs for translation currency with StartCurrencyCode to currency with EndCurrencyCode
    pub exchange_id_currency_pair_settings: Vec<ExchangeIdCurrencyPairSettings>,
}

impl CurrencyPriceSourceSettings {
    pub fn new(
        start_currency_code: CurrencyCode,
        end_currency_code: CurrencyCode,
        exchange_id_currency_pair_settings: Vec<ExchangeIdCurrencyPairSettings>,
    ) -> Self {
        Self {
            start_currency_code,
            end_currency_code,
            exchange_id_currency_pair_settings,
        }
    }
}

pub struct ExchangeIdCurrencyPairSettings {
    pub exchange_account_id: ExchangeAccountId,
    pub currency_pair: CurrencyPair,
}
