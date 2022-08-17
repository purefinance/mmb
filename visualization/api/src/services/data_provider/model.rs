use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

pub type ExchangeId = String;
pub type CurrencyCode = String;
pub type Amount = Decimal;
pub type Price = Decimal;

#[derive(sqlx::FromRow, Serialize, Clone)]
pub(crate) struct EventRecord {
    pub id: i64,
    pub json: Value,
}
