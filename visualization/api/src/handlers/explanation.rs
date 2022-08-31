use crate::services::data_provider::explanation::{Explanation, ExplanationService};
use actix_web::web::Data;
use actix_web::{get, web, Error, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationQuery {
    exchange_name: String,
    currency_code_pair: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationsGetResponse {
    exchange_name: String,
    currency_code_pair: String,
    explanations: Vec<Explanation>,
}

#[get("")]
pub async fn get(
    query: web::Query<ExplanationQuery>,
    explanation_service: Data<Arc<ExplanationService>>,
) -> Result<HttpResponse, Error> {
    let explanations = explanation_service
        .list(&query.exchange_name, &query.currency_code_pair, 300)
        .await;
    match explanations {
        Ok(explanations) => {
            let response = ExplanationsGetResponse {
                exchange_name: query.exchange_name.clone(),
                currency_code_pair: query.currency_code_pair.clone(),
                explanations,
            };
            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            log::error!("list explanation {e:?}");
            Ok(HttpResponse::InternalServerError().finish())
        }
    }
}
