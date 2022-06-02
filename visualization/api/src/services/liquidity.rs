use crate::ws::subscribes::liquidity::LiquiditySubscription;
use rand::Rng;
use serde_json::{json, Value};
use std::collections::HashSet;

/// Data Provider for Liquidity
#[derive(Default, Clone)]
pub struct LiquidityService;

#[derive(Clone)]
pub struct LiquidityData {
    pub exchange_id: String,
    pub currency_pair: String,
    pub record: LiquidityRecord,
}
#[derive(Clone)]
pub struct LiquidityRecord {
    pub data: Value,
}

impl LiquidityService {
    pub fn get_random_liquidity_data_by_subscriptions(
        &self,
        subscriptions: HashSet<LiquiditySubscription>,
    ) -> Vec<LiquidityData> {
        let mut result: Vec<LiquidityData> = vec![];
        for sub in subscriptions {
            let record = self.get_random_liquidity_data(&sub.exchange_id, &sub.currency_pair);
            let liquidity_data = LiquidityData {
                exchange_id: sub.exchange_id,
                currency_pair: sub.currency_pair,
                record,
            };
            result.push(liquidity_data);
        }
        result
    }
}

impl LiquidityService {
    pub fn get_random_liquidity_data(
        &self,
        exchange_id: &str,
        currency_pair: &str,
    ) -> LiquidityRecord {
        let data = json!({
            "ordersStateAndTransactions": {
                "exchangeName": exchange_id,
                "currencyCodePair": currency_pair,
                "desiredAmount": 0,
                "sell": {
                    "orders": [
                    //     {"amount":&self.get_random_int(), "price": &self.get_random_float()},
                    //     {"amount":&self.get_random_int(), "price": &self.get_random_float()},
                    //     {"amount":&self.get_random_int(), "price": &self.get_random_float()},
                    ],
                    "snapshot": [
                        [&self.get_random_float(), &self.get_random_int()],
                        [&self.get_random_float(), &self.get_random_int()],
                        [&self.get_random_float(), &self.get_random_int()]]
                },
                "buy": {
                    "orders": [
                    //     {"amount": &self.get_random_int(), "price": &self.get_random_float()},
                    //     {"amount": &self.get_random_int(), "price": &self.get_random_float()},
                    //     {"amount": &self.get_random_int(), "price": &self.get_random_float()},
                    ],
                    "snapshot": [
                        [&self.get_random_float(), &self.get_random_int()],
                        [&self.get_random_float(), &self.get_random_int()],
                        [&self.get_random_float(), &self.get_random_int()]
                    ]
                },
                "transactions": [{
                    "id": 1,
                    "dateTime": "2005-08-09T18:31:42",
                    "price": &self.get_random_float(),
                    "amount": &self.get_random_int(),
                    "hedged": 1,
                    "profitLossPct": &self.get_random_float(),
                    "status": "Finished",
                    "trades": [{
                        "exchangeName": exchange_id,
                        "dateTime": "2005-08-09T18:31:42",
                        "price": &self.get_random_float(),
                        "amount": &self.get_random_int(),
                        "exchangeOrderId": "1",
                        "direction": 0
                    },{
                        "exchangeName": exchange_id,
                        "dateTime": "2005-08-09T18:32:42",
                        "price": &self.get_random_float(),
                        "amount": &self.get_random_int(),
                        "exchangeOrderId": "2",
                        "direction": 0
                    },{
                        "exchangeName": exchange_id,
                        "dateTime": "2005-08-09T18:33:42",
                        "price": &self.get_random_float(),
                        "amount": &self.get_random_int(),
                        "exchangeOrderId": "3",
                        "direction": 1
                    }
                    ]
                }],

            },
        });
        LiquidityRecord { data }
    }

    // temporary methods
    fn get_random_float(&self) -> f64 {
        let mut rng = rand::thread_rng();
        f64::trunc(rng.gen::<f64>() * 100.0) / 100.0
    }

    fn get_random_int(&self) -> u8 {
        let mut rng = rand::thread_rng();
        rng.gen::<u8>()
    }
}
