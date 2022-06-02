use actix_web::get;
use actix_web::http::Error;
use actix_web::HttpResponse;
use serde_json::json;

#[get("/supported-exchanges")]
pub async fn supported_exchanges() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(json!({
        "supportedExchanges": [
            {"name": "Acx",
            "symbols": [
                {   "currencyCodePair": "BTC-USD",
                    "currencyPair": "BTC-USD"
                },
                {   "currencyCodePair": "BTC-EUR",
                    "currencyPair": "BTC-EUR"
                },
            ]
            },
            {"name": "Aac",
            "symbols": [
                {   "currencyCodePair": "BTC-USD",
                    "currencyPair": "BTC-USD"
                },
                {   "currencyCodePair": "BTC-RUB",
                    "currencyPair": "BTC-RUB"
                },
            ]
            }
        ]
    })))
}
