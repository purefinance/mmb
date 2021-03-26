// Get data to access binance account
#[macro_export]
macro_rules! get_binance_credentials {
    () => {{
        let api_key = env::var("BINANCE_API_KEY");
        if api_key.is_err() {
            dbg!("Environment variable BINANCE_API_KEY are not set. Unable to continue test");
            return;
        }

        let secret_key = env::var("BINANCE_SECRET_KEY");
        if secret_key.is_err() {
            dbg!("Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test");
            return;
        }

        (api_key, secret_key)
    }};
}
