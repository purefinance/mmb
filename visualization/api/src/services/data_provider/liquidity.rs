use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

use mmb_domain::order::snapshot::{Amount, Price};

use crate::services::data_provider::model::EventRecord;
use crate::types::{CurrencyPair, ExchangeId};
use serde_aux::prelude::deserialize_number_from_string;

/// Data Provider for Liquidity
#[derive(Clone)]
pub struct LiquidityService {
    pool: Pool<Postgres>,
}

#[derive(Clone)]
pub struct LiquidityData {
    pub order_book: OrderBookRecord,
    pub transactions: Vec<TransactionRecord>,
    pub desired_amount: Amount,
}

#[derive(Deserialize, Clone)]
pub struct OrderBookRecord {
    pub snapshot: OrderBookSnapshotRecord,
    pub orders: Vec<LiquidityOrderRecord>,
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
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
    pub exchange_id: ExchangeId,
    pub currency_pair: CurrencyPair,
}

#[derive(Deserialize, Clone)]
pub struct TransactionTradesRecord {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub price: Price,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub amount: Amount,
    pub exchange_id: ExchangeId,
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

    pub async fn get_liquidity_data(
        &self,
        exchange_id: &ExchangeId,
        currency_pair: &CurrencyPair,
        transaction_limit: i32,
    ) -> Result<LiquidityData, sqlx::Error> {
        let order_book = self.get_order_book(exchange_id, currency_pair).await?;

        let transactions = self
            .get_transactions(exchange_id, currency_pair, transaction_limit)
            .await?;

        Ok(LiquidityData {
            order_book,
            transactions,
            desired_amount: Default::default(),
        })
    }
}

impl LiquidityService {
    pub async fn get_order_book(
        &self,
        exchange_id: &ExchangeId,
        currency_pair: &CurrencyPair,
    ) -> Result<OrderBookRecord, sqlx::Error> {
        let sql = include_str!("../sql/get_order_book.sql");
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
        exchange_id: &ExchangeId,
        currency_pair: &CurrencyPair,
        limit: i32,
    ) -> Result<Vec<TransactionRecord>, sqlx::Error> {
        let sql = include_str!("../sql/get_transactions.sql");
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
