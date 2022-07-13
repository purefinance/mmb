use crate::services::market_settings::MarketSettingsService;
use actix_web::get;
use actix_web::http::Error;
use actix_web::web::Data;
use actix_web::HttpResponse;
use itertools::Itertools;
use serde_json::json;
use std::sync::Arc;

#[get("/supported-exchanges")]
pub async fn supported_exchanges(
    market_settings_service: Data<Arc<MarketSettingsService>>,
) -> Result<HttpResponse, Error> {
    let supported_exchanges = market_settings_service
        .exchanges
        .iter()
        .map(|it| {
            let symbols =
                it.1.iter()
                    .map(|it| {
                        json!({
                            "currencyCodePair": it.0,
                            "currencyPair": it.0.to_uppercase()
                        })
                    })
                    .collect_vec();
            json!({
                "name": it.0,
                "symbols": symbols
            })
        })
        .collect_vec();

    Ok(HttpResponse::Ok().json(json!({ "supportedExchanges": supported_exchanges })))
}
