use std::sync::Arc;

use actix_web::web::Data;
use paperclip::actix::{api_v2_operation, web::Json, Apiv2Schema, NoContent};
use serde::{Deserialize, Serialize};
use toml::Value;

use crate::error::AppError;
use crate::services::settings::{SettingCodes, SettingsService};

#[derive(Deserialize, Apiv2Schema)]
pub struct ConfigPayload {
    config: String,
}

#[derive(Serialize, Apiv2Schema)]
pub struct GetConfigResponse {
    config: Option<String>,
}

#[api_v2_operation(tags(Configuration))]
pub async fn get(
    settings_service: Data<Arc<SettingsService>>,
) -> Result<Json<GetConfigResponse>, AppError> {
    let configuration = settings_service
        .get_settings(SettingCodes::Configuration)
        .await;
    match configuration {
        Ok(configuration) => Ok(Json(GetConfigResponse {
            config: configuration.content,
        })),
        Err(e) => match e {
            sqlx::Error::RowNotFound => Ok(Json(GetConfigResponse { config: None })),
            _ => {
                log::error!("Get config error: {:?}", e);
                Err(AppError::InternalServerError)
            }
        },
    }
}

#[api_v2_operation(tags(Configuration))]
pub async fn save(
    payload: Json<ConfigPayload>,
    settings_service: Data<Arc<SettingsService>>,
) -> Result<NoContent, AppError> {
    if toml::from_str::<Value>(&payload.config).is_err() {
        return Err(AppError::BadRequest);
    }
    match settings_service
        .save_setting(SettingCodes::Configuration, &payload.config)
        .await
    {
        Ok(()) => Ok(NoContent),
        Err(e) => {
            log::error!("Save config error: {e:?}. Config {}", &payload.config);
            Err(AppError::InternalServerError)
        }
    }
}

#[derive(Serialize, Apiv2Schema)]
pub struct ValidateResponse {
    valid: bool,
    error: Option<String>,
}

#[api_v2_operation(tags(Configuration))]
pub async fn validate(payload: Json<ConfigPayload>) -> Json<ValidateResponse> {
    let response = match toml::from_str::<Value>(&payload.config) {
        Ok(_) => ValidateResponse {
            valid: true,
            error: None,
        },
        Err(e) => ValidateResponse {
            valid: false,
            error: Some(e.to_string()),
        },
    };
    Json(response)
}
