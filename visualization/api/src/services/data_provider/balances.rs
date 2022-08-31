use crate::services::data_provider::model::{CurrencyCode, EventRecord, ExchangeId};
use domain::order::snapshot::Amount;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;

/// Data Provider for Balances
#[derive(Clone)]
pub struct BalancesService {
    pool: Pool<Postgres>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BalancesRecord {
    pub balances_by_exchange_id: Option<HashMap<ExchangeId, HashMap<CurrencyCode, Decimal>>>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BalanceData {
    pub exchange_id: ExchangeId,
    pub currency_code: CurrencyCode,
    pub value: Amount,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BalancesData {
    pub balances: Vec<BalanceData>,
}

impl BalancesService {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn get_balances(&self) -> Result<BalancesData, sqlx::Error> {
        let sql = include_str!("../sql/get_last_balances.sql");
        let record = sqlx::query_as::<Postgres, EventRecord>(sql)
            .fetch_one(&self.pool)
            .await?;

        let result: BalancesRecord = serde_json::from_value(record.json)
            .unwrap_or_else(|_| panic!("Incorrect database balances data. ID: {:?}", record.id));
        let mut exchange_balances: Vec<BalanceData> = vec![];

        match result.balances_by_exchange_id {
            None => {}
            Some(hashmap) => hashmap.into_iter().for_each(|it| {
                it.1.into_iter().for_each(|it2| {
                    let balance_data = BalanceData {
                        exchange_id: it.0.clone(),
                        currency_code: it2.0,
                        value: it2.1,
                    };
                    exchange_balances.push(balance_data);
                })
            }),
        }

        exchange_balances.sort_unstable_by(|a, b| {
            (&a.exchange_id, &a.currency_code).cmp(&(&b.exchange_id, &b.currency_code))
        });

        Ok(BalancesData {
            balances: exchange_balances,
        })
    }
}
