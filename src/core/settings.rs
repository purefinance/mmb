use crate::core::exchanges::common::{CurrencyCode, ExchangeAccountId};

#[derive(Debug, Default, Clone)]
pub struct AppSettings<TBotSettings: Clone> {
    pub bot: TBotSettings,
    pub core: CoreSettings,
}

#[derive(Debug, Default, Clone)]
pub struct CoreSettings {
    pub exchanges: Vec<ExchangeSettings>,
}

#[derive(Debug, Clone)]
pub struct CurrencyPairSetting {
    pub base: CurrencyCode,
    pub quote: CurrencyCode,
    // currency code specific for exchange
    pub currency_pair: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExchangeSettings {
    pub exchange_account_id: ExchangeAccountId,
    // TODO add other settings
    pub api_key: String,
    pub secret_key: String,
    pub is_marging_trading: bool,
    // TODO change String to URI
    pub web_socket_host: String,
    // Some exchanges have two websockets, for public and private data
    pub web_socket2_host: String,
    pub rest_host: String,
    pub websocket_channels: Vec<String>,
    pub currency_pairs: Option<Vec<CurrencyPairSetting>>,
    _private: (), // field base constructor shouldn't be accessible from other modules
}

impl ExchangeSettings {
    pub fn new(
        exchange_account_id: ExchangeAccountId,
        api_key: String,
        secret_key: String,
        is_marging_trading: bool,
    ) -> Self {
        Self {
            exchange_account_id,
            api_key,
            secret_key,
            is_marging_trading,
            web_socket_host: "".into(),
            web_socket2_host: "".into(),
            rest_host: "".into(),
            websocket_channels: vec![],
            currency_pairs: None,
            _private: (),
        }
    }
}

impl Default for ExchangeSettings {
    fn default() -> Self {
        ExchangeSettings {
            exchange_account_id: ExchangeAccountId::new("".into(), 0),
            api_key: "".to_string(),
            secret_key: "".to_string(),
            is_marging_trading: false,
            web_socket_host: "".to_string(),
            web_socket2_host: "".to_string(),
            rest_host: "".to_string(),
            websocket_channels: vec![],
            currency_pairs: None,
            _private: (),
        }
    }
}
