use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct EventRecord {
    pub id: i64,
    pub json: Value,
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct EventTimedRecord {
    pub id: i64,
    pub insert_time: DateTime<chrono::Utc>,
    pub json: Value,
}
