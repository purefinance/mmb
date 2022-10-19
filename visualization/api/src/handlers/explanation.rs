use std::sync::Arc;

use actix_web::web::Data;
use paperclip::actix::{
    api_v2_operation,
    web::{self, Json},
    Apiv2Schema,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::data_provider::explanation::{Explanation, ExplanationService};

#[derive(Deserialize, Apiv2Schema)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationQuery {
    exchange_name: String,
    currency_code_pair: String,
}

#[derive(Serialize, Apiv2Schema)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationsGetResponse {
    exchange_name: String,
    currency_code_pair: String,
    explanations: Vec<Explanation>,
}

#[api_v2_operation(tags(Explanation), summary = "Get Explanations")]
pub async fn get(
    query: web::Query<ExplanationQuery>,
    explanation_service: Data<Arc<ExplanationService>>,
) -> Result<Json<ExplanationsGetResponse>, AppError> {
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
            Ok(Json(response))
        }
        Err(e) => {
            log::error!("list explanation {e:?}");
            Err(AppError::InternalServerError)
        }
    }
}
