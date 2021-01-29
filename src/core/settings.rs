pub struct ExchangeSettings {
    // TODO add other settings
    pub api_key: String,
    pub secret_key: String,

    pub is_marging_trading: bool,
    // TODO Why all of these are String, not http::uri?
    pub web_socket_host: String,
    pub web_socket2_host: String,
    pub rest_host: String,
}
