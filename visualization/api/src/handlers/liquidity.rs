use actix_web::get;
use actix_web::http::Error;
use actix_web::HttpResponse;
use serde_json::json;

#[get("/supported-exchanges")]
pub async fn supported_exchanges() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(json!({
        "supportedExchanges": [
            {"name": "Binance",
            "symbols": [
                {   "currencyCodePair": "btc/usdt",
                    "currencyPair": "BTC/USDT"
                },
                {   "currencyCodePair": "eth/btc",
                    "currencyPair": "ETH/BTC"
                },
            ]
            }
        ]
    })))
}
