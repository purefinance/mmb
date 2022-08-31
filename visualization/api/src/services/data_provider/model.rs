use serde::Serialize;
use serde_json::Value;

pub type ExchangeId = String;
pub type CurrencyCode = String;

#[derive(sqlx::FromRow, Serialize, Clone)]
pub(crate) struct EventRecord {
    pub id: i64,
    pub json: Value,
}
