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
}
