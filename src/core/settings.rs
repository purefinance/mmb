#[derive(Debug, Default)]
pub struct CoreSettings {}

#[derive(Debug, Default)]
pub struct ExchangeSettings {
    // TODO add other settings
    pub api_key: String,
    pub secret_key: String,
    pub is_marging_trading: bool,
    // TODO change String to URI
    pub web_socket_host: String,
    // Some exchanges have two websockets, for public and private data
    pub web_socket2_host: String,
    pub rest_host: String,
    _private: (), // field base constructor shouldn't be accessible from other modules
}

impl ExchangeSettings {
    pub fn new(api_key: String, secret_key: String, is_marging_trading: bool) -> Self {
        Self {
            api_key,
            secret_key,
            is_marging_trading,
            web_socket_host: "".into(),
            web_socket2_host: "".into(),
            rest_host: "".into(),
            _private: (),
        }
    }
}
