use std::sync::Arc;

use actix_web::web::Data;
use paperclip::actix::{api_v2_operation, web::Json};
use serde_json::{json, Value};

use crate::services::market_settings::MarketSettingsService;

#[api_v2_operation(tags(Liquidity), summary = "Get supported exchanges")]
pub async fn supported_exchanges(
    market_settings_service: Data<Arc<MarketSettingsService>>,
) -> Json<Value> {
    Json(json!({ "supportedExchanges": &market_settings_service.supported_exchanges }))
}
