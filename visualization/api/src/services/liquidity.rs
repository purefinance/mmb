use rand::Rng;
use serde_json::{json, Value};

/// Data Provider for Liquidity
#[derive(Default)]
pub struct LiquidityService;

impl LiquidityService {
    pub fn get_random_liquidity_data(&self) -> Value {
        json!({
            "ordersStateAndTransactions": {
                "exchangeName": "Acx",
                "currencyCodePair": "BTC-USD",
                "desiredAmount": 0,
                "sell": {
                    "orders": [
                        {"amount":&self.get_random_int(), "price": &self.get_random_float()},
                        {"amount":&self.get_random_int(), "price": &self.get_random_float()},
                        {"amount":&self.get_random_int(), "price": &self.get_random_float()},
                    ],
                    "snapshot": [
                        [&self.get_random_float(), &self.get_random_int()],
                        [&self.get_random_float(), &self.get_random_int()],
                        [&self.get_random_float(), &self.get_random_int()]]
                },
                "buy": {
                    "orders": [
                        {"amount": &self.get_random_int(), "price": &self.get_random_float()},
                        {"amount": &self.get_random_int(), "price": &self.get_random_float()},
                        {"amount": &self.get_random_int(), "price": &self.get_random_float()},
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
                        "exchangeName": "Acx",
                        "dateTime": "2005-08-09T18:31:42",
                        "price": &self.get_random_float(),
                        "amount": &self.get_random_int(),
                        "exchangeOrderId": "1",
                        "direction": 0
                    },{
                        "exchangeName": "Acx",
                        "dateTime": "2005-08-09T18:32:42",
                        "price": &self.get_random_float(),
                        "amount": &self.get_random_int(),
                        "exchangeOrderId": "2",
                        "direction": 0
                    },{
                        "exchangeName": "Acx",
                        "dateTime": "2005-08-09T18:33:42",
                        "price": &self.get_random_float(),
                        "amount": &self.get_random_int(),
                        "exchangeOrderId": "3",
                        "direction": 1
                    }
                    ]
                }],

            },
        })
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
