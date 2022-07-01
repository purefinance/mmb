use crate::ws::subscribes::liquidity::LiquiditySubscription;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_aux::prelude::*;
use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::collections::HashSet;

pub type Amount = Decimal;
pub type Price = Decimal;
/// Data Provider for Liquidity
#[derive(Clone)]
pub struct LiquidityService {
    pool: Pool<Postgres>,
}

#[derive(Clone)]
pub struct LiquidityData {
    pub exchange_id: String,
    pub currency_pair: String,
    pub order_book: OrderBookRecord,
    pub transactions: Vec<TransactionRecord>,
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct EventRecord {
    pub id: i64,
    pub json: Value,
}

#[derive(Deserialize, Clone)]
pub struct OrderBookRecord {
    pub snapshot: OrderBookSnapshotRecord,
    pub orders: Vec<LiquidityOrderRecord>,
    pub exchange_id: String,
    pub currency_pair: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiquidityOrderRecord {
    pub client_order_id: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub price: Price,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: Amount,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub remaining_amount: Amount,
    pub side: LiquidityOrderSide,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
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
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub price: Price,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: Amount,
}

#[derive(Deserialize, Clone)]
pub struct TransactionRecord {
    pub side: TransactionOrderSide,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub price: Price,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: Amount,
    pub hedged: Option<String>,
    pub status: String,
    pub revision: i64,
    pub strategy_name: String,
    pub transaction_id: String,
    pub profit_loss_pct: Option<String>,
    pub transaction_creation_time: String,
    pub trades: Vec<TransactionTradesRecord>,
    pub market_id: MarketIdRecord,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum TransactionOrderSide {
    Buy,
    Sell,
}

#[derive(Deserialize, Clone)]
pub struct MarketIdRecord {
    pub exchange_id: String,
    pub currency_pair: String,
}

#[derive(Deserialize, Clone)]
pub struct TransactionTradesRecord {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub price: Price,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: Amount,
    pub exchange_id: String,
    pub exchange_order_id: String,
    pub side: Option<TransactionTradeSide>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum TransactionTradeSide {
    Buy,
    Sell,
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
            let order_book = self
                .get_order_book(&sub.exchange_id, &sub.currency_pair)
                .await?;

            let transactions = self
                .get_transactions(&sub.exchange_id, &sub.currency_pair, 20)
                .await?;

            let liquidity_data = LiquidityData {
                exchange_id: sub.exchange_id.clone(),
                currency_pair: sub.currency_pair.clone(),
                order_book,
                transactions,
            };
            result.push(liquidity_data);
        }
        Ok(result)
    }
}

impl LiquidityService {
    pub async fn get_order_book(
        &self,
        exchange_id: &str,
        currency_pair: &str,
    ) -> Result<OrderBookRecord, sqlx::Error> {
        let sql = include_str!("sql/get_order_book.sql");
        let record = sqlx::query_as::<Postgres, EventRecord>(sql)
            .bind(exchange_id)
            .bind(currency_pair)
            .fetch_one(&self.pool)
            .await?;

        let result: OrderBookRecord = serde_json::from_value(record.json)
            .unwrap_or_else(|_| panic!("Incorrect database order book data. ID: {:?}", record.id));
        Ok(result)
    }

    pub async fn get_transactions(
        &self,
        exchange_id: &str,
        currency_pair: &str,
        limit: i32,
    ) -> Result<Vec<TransactionRecord>, sqlx::Error> {
        let sql = include_str!("sql/get_transactions.sql");
        let records = sqlx::query_as::<Postgres, EventRecord>(sql)
            .bind(exchange_id)
            .bind(currency_pair)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(records
            .into_iter()
            .map(|r| {
                serde_json::from_value(r.json).unwrap_or_else(|_| {
                    panic!("Incorrect database transaction data. ID: {:?}", r.id)
                })
            })
            .collect())
    }
}
