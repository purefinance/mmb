use crate::services::settings::{SettingCodes, SettingsService};
use actix_web::web::Data;
use actix_web::{get, post, put, web, Error, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use toml::Value;

#[derive(Deserialize)]
pub struct ConfigPayload {
    config: String,
}

#[derive(Serialize)]
pub struct GetConfigResponse {
    config: Option<String>,
}

#[get("")]
pub async fn get(settings_service: Data<Arc<SettingsService>>) -> Result<HttpResponse, Error> {
    let configuration = settings_service
        .get_settings(SettingCodes::Configuration)
        .await;
    match configuration {
        Ok(configuration) => Ok(HttpResponse::Ok().json(GetConfigResponse {
            config: configuration.content,
        })),
        Err(e) => match e {
            sqlx::Error::RowNotFound => {
                Ok(HttpResponse::Ok().json(GetConfigResponse { config: None }))
            }
            _ => {
                log::error!("Get config error: {:?}", e);
                Ok(HttpResponse::InternalServerError().finish())
            }
        },
    }
}

#[put("")]
pub async fn save(
    payload: web::Json<ConfigPayload>,
    settings_service: Data<Arc<SettingsService>>,
) -> Result<HttpResponse, Error> {
    if toml::from_str::<Value>(&payload.config).is_err() {
        return Ok(HttpResponse::BadRequest().finish());
    }
    match settings_service
        .save_setting(SettingCodes::Configuration, &payload.config)
        .await
    {
        Ok(()) => Ok(HttpResponse::Ok().finish()),
        Err(e) => {
            log::error!("Save config error: {:?}. Config {}", e, &payload.config);
            Ok(HttpResponse::InternalServerError().finish())
        }
    }
}

#[derive(Serialize)]
pub struct ValidateResponse {
    valid: bool,
    error: Option<String>,
}

#[post("/validate")]
pub async fn validate(
    payload: web::Json<ConfigPayload>,
) -> Result<web::Json<ValidateResponse>, Error> {
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

    Ok(web::Json(response))
}
