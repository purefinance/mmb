use crate::exchanges::common::{Amount, CurrencyCode, CurrencyPair, ExchangeAccountId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub trait BaseStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId;
    fn currency_pair(&self) -> CurrencyPair;
    fn max_amount(&self) -> Amount;
}

/// Application settings
/// Attention! After changing in runtime, you need to save the settings. See issue #146
/// For the settings to be applied, the trading engine must be restarted after changing the config
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AppSettings<StrategySettings>
where
    StrategySettings: BaseStrategySettings + Clone,
{
    pub strategy: StrategySettings,
    pub core: CoreSettings,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CoreSettings {
    pub database: Option<DbSettings>,
    pub exchanges: Vec<ExchangeSettings>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DbSettings {
    pub url: String,
    pub migrations: Vec<PathBuf>,
    /// Path to directory for creating temporary directory for save events that was not saved to
    /// database by any reason and will be resaved to db late
    pub postponed_events_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CurrencyPairSetting {
    Ordinary {
        base: CurrencyCode,
        quote: CurrencyCode,
    },
    Specific(String),
}

// Field order are matter for serialization:
// Simple values must be emitted before struct with custom serialization
// https://github.com/alexcrichton/toml-rs/issues/142#issuecomment-278970591
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

impl Default for ExchangeSettings {
    fn default() -> Self {
        ExchangeSettings {
            exchange_account_id: ExchangeAccountId::new("", 0),
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

pub enum TimePeriodKind {
    Hour,
    Day,
}

pub struct StopperCondition {
    pub period_kind: TimePeriodKind,
    pub period_value: i64,
    pub limit: Amount,
}

pub struct ProfitLossStopperSettings {
    pub conditions: Vec<StopperCondition>,
}
