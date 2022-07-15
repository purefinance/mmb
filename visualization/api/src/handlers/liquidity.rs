use crate::services::market_settings::MarketSettingsService;
use actix_web::get;
use actix_web::http::Error;
use actix_web::web::Data;
use actix_web::HttpResponse;
use serde_json::json;
use std::sync::Arc;

#[get("/supported-exchanges")]
pub async fn supported_exchanges(
    market_settings_service: Data<Arc<MarketSettingsService>>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok()
        .json(json!({ "supportedExchanges": &market_settings_service.supported_exchanges })))
}
