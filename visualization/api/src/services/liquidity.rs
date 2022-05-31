use rand::Rng;
use serde_json::{json, Value};

/// Data Provider for Liquidity
#[derive(Default)]
pub struct LiquidityService;

impl LiquidityService {
    pub fn get_random_liquidity_data(&self) -> Value {
        let mut rng = rand::thread_rng();

        json!({
            "ordersStateAndTransactions": {
                "exchangeName": "$exchangeName",
                "currencyCodePair": "$currencyCodePair",
                "desiredAmount": 0,
                "sell": {
                    "orders": [
                        {"amount":rng.gen::<u8>(), "price": rng.gen::<f64>()},
                        {"amount":rng.gen::<u8>(), "price": rng.gen::<f64>()},
                        {"amount":rng.gen::<u8>(), "price": rng.gen::<f64>()},
                    ],
                    "snapshot": [
                        [rng.gen::<f64>(), rng.gen::<u8>()],
                        [rng.gen::<f64>(), rng.gen::<u8>()],
                        [rng.gen::<f64>(), rng.gen::<u8>()]]
                },
                "buy": {
                    "orders": [
                        {"amount": rng.gen::<u8>(), "price": rng.gen::<f64>()},
                        {"amount": rng.gen::<u8>(), "price": rng.gen::<f64>()},
                        {"amount": rng.gen::<u8>(), "price": rng.gen::<f64>()},
                    ],
                    "snapshot": [
                        [rng.gen::<f64>(), rng.gen::<u8>()],
                        [rng.gen::<f64>(), rng.gen::<u8>()],
                        [rng.gen::<f64>(), rng.gen::<u8>()]
                    ]
                },
                "transactions": [{
                    "id": 1,
                    "dateTime": "2005-08-09T18:31:42",
                    "price": rng.gen::<f32>(),
                    "amount": rng.gen::<u8>(),
                    "hedged": 1,
                    "profitLossPct": rng.gen::<f64>(),
                    "trades": [{
                        "exchangeName": "$exchangeName",
                        "dateTime": "2005-08-09T18:31:42",
                        "price": rng.gen::<f32>(),
                        "amount": rng.gen::<u8>(),
                        "exchangeOrderId": "1",
                        "direction": 0
                    }]
                }]
            },
        })
    }
}
