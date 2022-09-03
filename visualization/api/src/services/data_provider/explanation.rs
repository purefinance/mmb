use chrono::DateTime;
use itertools::Itertools;
use mmb_domain::order::snapshot::{Amount, Price};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

use crate::services::data_provider::model::EventTimedRecord;
use crate::types::{CurrencyPair, ExchangeId};

/// Data Provider for Explanations
#[derive(Clone)]
pub struct ExplanationService {
    pool: Pool<Postgres>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(deserialize = "snake_case", serialize = "camelCase"))]
pub struct PriceLevelRecord {
    pub price: Price,
    pub amount: Amount,
    pub reasons: Vec<String>,
    pub mode_name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(deserialize = "snake_case", serialize = "camelCase"))]
pub struct ExplanationRecord {
    pub set: Vec<PriceLevelRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(deserialize = "snake_case", serialize = "camelCase"))]
pub struct Explanation {
    pub id: i64,
    pub date_time: DateTime<chrono::Utc>,
    pub price_levels: Vec<PriceLevelRecord>,
}

impl ExplanationService {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        exchange_id: &ExchangeId,
        currency_pair: &CurrencyPair,
        limit: i32,
    ) -> anyhow::Result<Vec<Explanation>> {
        let sql = include_str!("../sql/get_explanations.sql");
        let record = sqlx::query_as::<Postgres, EventTimedRecord>(sql)
            .bind(exchange_id)
            .bind(currency_pair)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let list = record
            .into_iter()
            .map(|it| {
                let record: ExplanationRecord =
                    serde_json::from_value(it.json).unwrap_or_else(|_| {
                        panic!("Incorrect database explanation json data. ID: {:?}", it.id)
                    });

                Explanation {
                    id: it.id,
                    date_time: it.insert_time,
                    price_levels: record.set,
                }
            })
            .collect_vec();
        Ok(list)
    }
}
