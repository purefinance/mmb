use crate::ws::subscribes::liquidity::LiquiditySubscription;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::collections::HashSet;

/// Data Provider for Liquidity
#[derive(Clone)]
pub struct LiquidityService {
    pool: Pool<Postgres>,
}

#[derive(Clone)]
pub struct LiquidityData {
    pub exchange_id: String,
    pub currency_pair: String,
    pub record: LiquidityRecord,
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct LiquidityJsonRecord {
    pub id: i64,
    pub json: Value,
}

#[derive(Deserialize, Clone)]
pub struct LiquidityRecord {
    pub snapshot: OrderBookSnapshotRecord,
    pub orders: Vec<LiquidityOrderRecord>,
    pub exchange_id: String,
    pub currency_pair: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiquidityOrderRecord {
    pub client_order_id: String,
    pub price: String,
    pub amount: String,
    pub remaining_amount: String,
    pub side: LiquidityOrderSide,
}

#[derive(Debug, Clone, Deserialize)]
pub enum LiquidityOrderSide {
    Buy,
    Sell,
}

#[derive(Deserialize, Clone)]
pub struct OrderBookSnapshotRecord {
    pub asks: Vec<PriceLevelRecord>,
    pub bids: Vec<PriceLevelRecord>,
}

#[derive(Deserialize, Clone)]
pub struct OrderBookOrderRecord;

#[derive(Deserialize, Clone)]
pub struct PriceLevelRecord {
    pub price: String,
    pub amount: String,
}

impl LiquidityService {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn get_liquidity_data_by_subscriptions(
        &self,
        subscriptions: &HashSet<LiquiditySubscription>,
    ) -> Result<Vec<LiquidityData>, sqlx::Error> {
        let mut result: Vec<LiquidityData> = vec![];
        for sub in subscriptions {
            let record = self
                .get_liquidity_data(&sub.exchange_id, &sub.currency_pair)
                .await?;
            let liquidity_data = LiquidityData {
                exchange_id: sub.exchange_id.clone(),
                currency_pair: sub.currency_pair.clone(),
                record,
            };
            result.push(liquidity_data);
        }
        Ok(result)
    }
}

impl LiquidityService {
    pub async fn get_liquidity_data(
        &self,
        exchange_id: &str,
        currency_pair: &str,
    ) -> Result<LiquidityRecord, sqlx::Error> {
        let sql = include_str!("sql/get_liquidity.sql");
        let json_record = sqlx::query_as::<Postgres, LiquidityJsonRecord>(sql)
            .bind(exchange_id)
            .bind(currency_pair)
            .fetch_one(&self.pool)
            .await?;

        let result: LiquidityRecord =
            serde_json::from_value(json_record.json).unwrap_or_else(|_| {
                panic!(
                    "Incorrect database liquidity data. ID: {:?}",
                    json_record.id
                )
            });

        Ok(result)
    }
}
