use std::io::Write;
use std::{fmt::Debug, fs::File, fs::OpenOptions};
use toml::value::Value;

use crate::core::settings::{AppSettings, BaseStrategySettings};
use anyhow::{anyhow, Result};
use itertools::Itertools;
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
    // Write credentials in their own config
    #[derive(Debug)]
    struct Credentials {
        exchange_account_id: String,
        api_key: String,
        secret_key: String,
    }

    let credentials_per_exchange = settings
        .core
        .exchanges
        .iter()
        .map(|exchange_settings| Credentials {
            exchange_account_id: exchange_settings.exchange_account_id.to_string(),
            api_key: exchange_settings.api_key.clone(),
            secret_key: exchange_settings.secret_key.clone(),
        })
        .collect_vec();

    let mut credentials_config = OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(credentials_path)?;
    for creds in credentials_per_exchange {
        credentials_config.write_all(format!("[{}]\n", creds.exchange_account_id).as_bytes())?;
        credentials_config.write_all(format!("api_key = \"{}\"\n", creds.api_key).as_bytes())?;
        credentials_config
            .write_all(format!("secret_key = \"{}\"\n\n", creds.secret_key).as_bytes())?;
    }

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
