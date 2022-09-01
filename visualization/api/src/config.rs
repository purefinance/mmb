use crate::types::ExchangeId;
use mmb_domain::order::snapshot::Amount;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AppConfig {
    pub address: String,
    pub database_url: String,
    pub refresh_data_interval_ms: u64,
    pub markets: Vec<Market>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Market {
    pub exchange_id: ExchangeId,
    pub info: Vec<MarketInfo>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MarketInfo {
    pub max_amount: Amount,
    pub currency_pair: String,
}

pub fn load_config(filepath: &str) -> AppConfig {
    let file_content = std::fs::read_to_string(filepath)
        .unwrap_or_else(|_| panic!("Failure open file {filepath}"));
    toml::from_str(&file_content).expect("Failure parse config file")
}
