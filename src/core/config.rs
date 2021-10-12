use std::{collections::HashMap, io::Write};
use std::{fmt::Debug, fs::File};
use toml;

use crate::{
    core::settings::{AppSettings, BaseStrategySettings},
    hashmap,
};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::io::Read;

pub static EXCHANGE_ACCOUNT_ID: &str = "exchange_account_id";
pub static API_KEY: &str = "api_key";
pub static SECRET_KEY: &str = "secret_key";
pub static CONFIG_PATH: &str = "config.toml";
pub static CREDENTIALS_PATH: &str = "credentials.toml";

pub fn load_settings<'a, TSettings>(
    config_path: &str,
    credentials_path: &str,
) -> Result<AppSettings<TSettings>>
where
    TSettings: BaseStrategySettings + Clone + Debug + Deserialize<'a>,
{
    let mut settings = String::new();
    File::open(config_path)?.read_to_string(&mut settings)?;

    let mut credentials = String::new();
    File::open(credentials_path)?.read_to_string(&mut credentials)?;

    parse_settings(&mut settings, &mut credentials)
}

pub fn parse_settings<'a, TSettings>(
    settings: &str,
    credentials: &str,
) -> Result<AppSettings<TSettings>>
where
    TSettings: BaseStrategySettings + Clone + Debug + Deserialize<'a>,
{
    let mut settings: toml::Value = toml::from_str(settings)?;

    let exchanges = get_exchanges_mut(&mut settings).ok_or(anyhow!(
        "Unable to get core.exchanges array from gotten settings"
    ))?;

    if !exchanges.is_empty() {
        let credentials: HashMap<&str, toml::Value> = toml::from_str(credentials)?;

        // Extract creds according to exchange_account_id and add it to every ExchangeSettings
        for exchange in exchanges {
            let exchange = exchange
                .as_table_mut()
                .ok_or(anyhow!("Unable access to exchange settings as table"))?;

            let exchange_account_id = exchange
                .get(EXCHANGE_ACCOUNT_ID)
                .and_then(|v| v.as_str())
                .ok_or(anyhow!(
                    "Unable get exchange account id for Exchange in settings"
                ))?;

            let api_key = credentials
                .get(exchange_account_id)
                .and_then(|v| v.get(API_KEY))
                .and_then(|v| v.as_str())
                .ok_or(anyhow!("Unable get api_key for Exchange in settings"))?;
            let secret_key = credentials
                .get(exchange_account_id)
                .and_then(|v| v.get(SECRET_KEY))
                .and_then(|v| v.as_str())
                .ok_or(anyhow!("Unable get secret_key for Exchange in settings"))?;

            exchange.insert(API_KEY.to_owned(), api_key.into());
            exchange.insert(SECRET_KEY.to_owned(), secret_key.into());
        }
    }

    settings
        .try_into()
        .context("Unable parse combined settings")
}

pub fn save_settings(settings: &str, config_path: &str, credentials_path: &str) -> Result<()> {
    let mut serialized_settings: toml::Value = toml::from_str(settings)?;
    // Write credentials in their own config file
    let mut credentials_per_exchange = HashMap::new();

    let exchanges = get_exchanges_mut(&mut serialized_settings).ok_or(anyhow!(
        "Unable to get core.exchanges array from gotten settings"
    ))?;
    for exchange_settings in exchanges {
        let exchange_settings = exchange_settings
            .as_table_mut()
            .ok_or(anyhow!("Unable to get mutable exchange table"))?;

        let (exchange_account_id, api_key, secret_key) =
            get_credentials_data(&exchange_settings)
                .ok_or(anyhow!("Unable to get credentials data for exchange"))?;

        let creds = hashmap![
            API_KEY => api_key,
            SECRET_KEY => secret_key
        ];

        credentials_per_exchange.insert(exchange_account_id, creds);

        // Remove credentials from main config
        let _ = exchange_settings.remove(API_KEY);
        let _ = exchange_settings.remove(SECRET_KEY);
    }

    let serialized_creds = toml::Value::try_from(credentials_per_exchange)?;
    let mut credentials_config = File::create(credentials_path)?;
    credentials_config.write_all(&serialized_creds.to_string().as_bytes())?;

    let mut main_config = File::create(config_path)?;
    main_config.write_all(&serialized_settings.to_string().as_bytes())?;

    Ok(())
}

fn get_credentials_data(
    exchange_settings: &toml::map::Map<String, toml::Value>,
) -> Option<(String, String, String)> {
    let exchange_account_id = exchange_settings
        .get(EXCHANGE_ACCOUNT_ID)?
        .as_str()?
        .to_owned();
    let api_key = exchange_settings.get(API_KEY)?.as_str()?.to_owned();
    let secret_key = exchange_settings.get(SECRET_KEY)?.as_str()?.to_owned();

    Some((exchange_account_id, api_key, secret_key))
}

fn get_exchanges_mut(serialized: &mut toml::Value) -> Option<&mut Vec<toml::Value>> {
    serialized
        .as_table_mut()?
        .get_mut("core")?
        .as_table_mut()?
        .get_mut("exchanges")?
        .as_array_mut()
}
