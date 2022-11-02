use std::fs::read_to_string;
use std::{collections::HashMap, io::Write};
use std::{fmt::Debug, fs::File};

use crate::lifecycle::launcher::InitSettings;
use crate::settings::{AppSettings, DispositionStrategySettings};
use anyhow::{anyhow, bail, Context, Result};
use mmb_utils::hashmap;
use mmb_utils::infrastructure::WithExpect;
use serde::de::DeserializeOwned;
use toml_edit::{value, ArrayOfTables, Document, Table};

pub static EXCHANGE_ACCOUNT_ID: &str = "exchange_account_id";
pub static API_KEY: &str = "api_key";
pub static SECRET_KEY: &str = "secret_key";
pub static CONFIG_PATH: &str = "config.toml";
pub static CREDENTIALS_PATH: &str = "credentials.toml";

pub fn try_load_settings<TSettings>(
    config_path: &str,
    credentials_path: &str,
) -> Result<AppSettings<TSettings>>
where
    TSettings: DispositionStrategySettings + Clone + Debug + DeserializeOwned,
{
    let settings = read_to_string(config_path)
        .with_context(|| format!("Unable load settings file: {}", config_path))?;
    let credentials = read_to_string(credentials_path)
        .with_context(|| format!("Unable load credentials file: {}", credentials_path))?;

    parse_settings(&settings, &credentials)
}

pub fn load_pretty_settings<StrategySettings>(
    init_user_settings: InitSettings<StrategySettings>,
) -> String
where
    StrategySettings: DispositionStrategySettings + Clone + serde::ser::Serialize,
{
    match init_user_settings {
        InitSettings::Directly(settings) => {
            toml_edit::ser::to_string(&settings).expect("Unable serialize user settings")
        }
        InitSettings::Load {
            config_path,
            credentials_path,
        } => {
            let settings = read_to_string(&config_path)
                .with_expect(|| format!("Unable load settings file: {}", config_path));
            let credentials = read_to_string(&credentials_path)
                .with_expect(|| format!("Unable load credentials file: {}", credentials_path));

            let settings =
                parse_toml_settings(&settings, &credentials).expect("Failed to parse toml file");
            settings.to_string()
        }
    }
}

pub fn parse_settings<TSettings>(
    settings: &str,
    credentials: &str,
) -> Result<AppSettings<TSettings>>
where
    TSettings: DispositionStrategySettings + Clone + Debug + DeserializeOwned,
{
    let settings =
        parse_toml_settings(settings, credentials).context("Unable parse toml settings")?;
    toml_edit::de::from_document::<AppSettings<TSettings>>(settings)
        .context("Unable parse combined settings")
}

pub fn save_settings(settings: &str, config_path: &str, credentials_path: &str) -> Result<()> {
    let mut serialized_settings: Document = settings.parse()?;

    // Write credentials in their own config file
    let mut credentials_per_exchange = HashMap::new();

    let exchanges = get_exchanges_mut(&mut serialized_settings)
        .ok_or_else(|| anyhow!("Unable to get core.exchanges array from gotten settings"))?;
    for exchange_settings in exchanges.iter_mut() {
        let (exchange_account_id, api_key, secret_key) = get_credentials_data(exchange_settings)
            .ok_or_else(|| anyhow!("Unable to get credentials data for exchange"))?;

        let creds = hashmap![
            API_KEY => api_key,
            SECRET_KEY => secret_key
        ];

        credentials_per_exchange.insert(exchange_account_id, creds);

        // Remove credentials from main config
        let _ = exchange_settings.remove(API_KEY);
        let _ = exchange_settings.remove(SECRET_KEY);
    }

    let serialized_creds = toml_edit::ser::to_string(&credentials_per_exchange)?;
    let mut credentials_config = File::create(credentials_path)?;
    credentials_config.write_all(serialized_creds.as_bytes())?;

    let mut main_config = File::create(config_path)?;
    main_config.write_all(serialized_settings.to_string().as_bytes())?;

    Ok(())
}

fn parse_toml_settings(settings: &str, credentials: &str) -> Result<Document> {
    let mut settings: Document = settings.parse().context("Unable parse settings")?;

    let exchanges = get_exchanges_mut(&mut settings)
        .context("Unable to get 'core.exchanges' array from gotten settings")?;

    if !exchanges.is_empty() {
        let credentials: Document = credentials.parse()?;
        let credentials = credentials.as_table();

        // Extract creds according to exchange_account_id and add it to every ExchangeSettings
        for exchange in exchanges.iter_mut() {
            let exchange_account_id = exchange
                .get(EXCHANGE_ACCOUNT_ID)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow!(
                "Unable get 'exchange_account_id' for one of 'core.exchanges' from the settings"
            )
                })?;

            let api_key = credentials
                .get(exchange_account_id)
                .and_then(|v| v.get(API_KEY))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow!("Unable get 'api_key' for one of 'core.exchanges' from the settings")
                })?;
            let secret_key = credentials
                .get(exchange_account_id)
                .and_then(|v| v.get(SECRET_KEY))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    anyhow!("Unable get 'secret_key' for one of 'core.exchanges' from the settings")
                })?;

            if api_key.is_empty() || secret_key.is_empty() {
                bail!("Unable to parse settings: api or secret key is empty")
            }

            exchange.insert(API_KEY, value(api_key));
            exchange.insert(SECRET_KEY, value(secret_key));
        }
    }

    Ok(settings)
}

fn get_credentials_data(exchange_settings: &Table) -> Option<(String, String, String)> {
    let exchange_account_id = exchange_settings
        .get(EXCHANGE_ACCOUNT_ID)?
        .as_str()?
        .to_owned();

    let api_key = exchange_settings.get(API_KEY)?.as_str()?.to_owned();
    let secret_key = exchange_settings.get(SECRET_KEY)?.as_str()?.to_owned();

    Some((exchange_account_id, api_key, secret_key))
}

fn get_exchanges_mut(serialized: &mut Document) -> Option<&mut ArrayOfTables> {
    serialized
        .as_table_mut()
        .get_mut("core")?
        .as_table_mut()?
        .get_mut("exchanges")?
        .as_array_of_tables_mut()
}
