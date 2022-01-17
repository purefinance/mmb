use std::str::FromStr;

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde_json::Value;

pub trait GetOrErr {
    fn get_as_str(&self, key: &str) -> Result<String>;
    fn get_as_decimal(&self, key: &str) -> Option<Decimal>;
}

impl GetOrErr for Value {
    fn get_as_str(&self, key: &str) -> Result<String> {
        Ok(self
            .get(key)
            .with_context(|| format!("Unable to get {} from {:?}", key, self))?
            .as_str()
            .with_context(|| format!("Unable to get {} as string", key))?
            .to_string())
    }

    fn get_as_decimal(&self, key: &str) -> Option<Decimal> {
        self.get(key)
            .and_then(|value| value.as_str())
            .and_then(|value| Decimal::from_str(value).ok())
    }
}
