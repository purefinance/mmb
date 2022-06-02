use crate::services::liquidity::{LiquidityData, LiquidityOrderSide};
use actix::prelude::*;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Message, Clone)]
#[rtype(result = "()")]
#[serde(rename_all = "camelCase")]
pub struct LiquidityResponseBody {
    pub orders_state_and_transactions: OrderStateAndTransactions,
}

impl From<LiquidityData> for LiquidityResponseBody {
    fn from(liquidity_data: LiquidityData) -> Self {
        let sell_shapshot = liquidity_data
            .record
            .snapshot
            .asks
            .into_iter()
            .map(|price_level| (price_level.amount, price_level.price))
            .collect_vec();
        let buy_snapshot = liquidity_data
            .record
            .snapshot
            .bids
            .into_iter()
            .map(|price_level| (price_level.amount, price_level.price))
            .collect_vec();

        let mut buy_orders: Vec<Order> = vec![];
        let mut sell_orders: Vec<Order> = vec![];

        liquidity_data
            .record
            .orders
            .into_iter()
            .for_each(|order| match order.side {
                LiquidityOrderSide::Buy => buy_orders.push(Order {
                    amount: order.amount,
                    price: order.price,
                }),
                LiquidityOrderSide::Sell => sell_orders.push(Order {
                    amount: order.amount,
                    price: order.price,
                }),
            });

        let state = OrderStateAndTransactions {
            exchange_name: liquidity_data.exchange_id,
            currency_code_pair: liquidity_data.currency_pair,
            desired_amount: 0.0,
            sell: Orders {
                orders: sell_orders,
                snapshot: sell_shapshot,
            },
            buy: Orders {
                orders: buy_orders,
                snapshot: buy_snapshot,
            },
            transactions: vec![],
        };

        Self {
            orders_state_and_transactions: state,
        }
    }
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
    pub snapshot: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Order {
    pub amount: String,
    pub price: String,
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
