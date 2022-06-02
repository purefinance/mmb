use actix::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Message, Clone)]
#[rtype(result = "()")]
#[serde(rename_all = "camelCase")]
pub struct LiquidityResponseBody {
    pub orders_state_and_transactions: OrderStateAndTransactions,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderStateAndTransactions {
    pub exchange_name: String,
    pub currency_code_pair: String,
    pub desired_amount: f64,
    pub sell: Orders,
    pub buy: Orders,
    pub transactions: Vec<Transaction>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Orders {
    pub orders: Vec<Order>,
    pub snapshot: Vec<(f64, u64)>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Order {
    pub amount: u64,
    pub price: f64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: u64,
    pub date_time: String,
    pub price: f64,
    pub amount: u64,
    pub hedged: u64,
    pub profit_loss_pct: f64,
    pub status: String,
    pub trades: Vec<Trade>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub exchange_name: String,
    pub date_time: String,
    pub price: f64,
    pub amount: u64,
    pub exchange_order_id: String,
    pub direction: u8,
}
