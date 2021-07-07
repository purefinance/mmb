use std::{collections::HashMap, io::Write};
use std::{fmt::Debug, fs::File};
use toml::value::Value;

use crate::{
    core::settings::{AppSettings, BaseStrategySettings},
    hashmap,
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub fn load_settings<'a, TSettings>(
    config_path: &str,
    credentials_path: &str,
) -> Result<AppSettings<TSettings>>
where
    TSettings: BaseStrategySettings + Clone + Debug + Deserialize<'a>,
{
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name(&config_path))?;
    let exchanges = settings.get_array("core.exchanges")?;

    let mut credentials = config::Config::default();
    credentials.merge(config::File::with_name(credentials_path))?;

    // Extract creds accoring to exchange_account_id and add it to every ExchangeSettings
    let mut exchanges_with_creds = Vec::new();
    for exchange in exchanges {
        let mut exchange = exchange.into_table()?;

        let exchange_account_id = exchange.get("exchange_account_id").ok_or(anyhow!(
            "Config file has no exchange account id for Exchange"
        ))?;
        let api_key = &credentials.get_str(&format!("{}.api_key", exchange_account_id))?;
        let secret_key = &credentials.get_str(&format!("{}.secret_key", exchange_account_id))?;

        exchange.insert("api_key".to_owned(), api_key.as_str().into());
        exchange.insert("secret_key".to_owned(), secret_key.as_str().into());

        exchanges_with_creds.push(exchange);
    }

    let mut config_with_creds = config::Config::new();
    config_with_creds.set("core.exchanges", exchanges_with_creds)?;

    settings.merge(config_with_creds)?;

    let decoded = settings.try_into()?;

    Ok(decoded)
}

pub fn save_settings<'a, TSettings>(
    settings: AppSettings<TSettings>,
    config_path: &str,
    credentials_path: &str,
) -> Result<()>
where
    TSettings: BaseStrategySettings + Clone + Debug + Deserialize<'a> + Serialize,
{
    // Write credentials in their own config file
    let mut credentials_per_exchange = HashMap::new();
    for exchange_settings in settings.core.exchanges.iter() {
        let creds = hashmap![
            "api_key" => exchange_settings.api_key.clone(),
            "secret_key" => exchange_settings.secret_key.clone()
        ];

        credentials_per_exchange.insert(exchange_settings.exchange_account_id.to_string(), creds);
    }

    let serialized_creds = Value::try_from(credentials_per_exchange)?;
    let mut credentials_config = File::create(credentials_path)?;
    credentials_config.write_all(&serialized_creds.to_string().as_bytes())?;

    // Remove credentials from main config
    let mut serialized = Value::try_from(settings)?;
    let exchanges = get_exchanges_mut(&mut serialized).ok_or(anyhow!(
        "Unable to get core.exchanges array from gotten settings"
    ))?;
    for exchange in exchanges {
        let exchange = exchange
            .as_table_mut()
            .ok_or(anyhow!("Unable to get mutable exchange table"))?;

        let _ = exchange.remove("api_key");
        let _ = exchange.remove("secret_key");
    }

    let mut main_config = File::create(config_path)?;
    main_config.write_all(&serialized.to_string().as_bytes())?;

    Ok(())
}

fn get_exchanges_mut(serialized: &mut Value) -> Option<&mut Vec<Value>> {
    serialized
        .as_table_mut()?
        .get_mut("core")?
        .as_table_mut()?
        .get_mut("exchanges")?
        .as_array_mut()
}
