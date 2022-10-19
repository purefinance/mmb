use actix_web::web::Data;
use paperclip::actix::{api_v2_operation, web::Json, Apiv2Schema};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::services::account::AccountService;
use crate::services::token::TokenService;

#[derive(Deserialize, Apiv2Schema)]
pub struct LoginPayload {
    username: String,
    password: String,
}

#[api_v2_operation(tags(Account), summary = "Authorization by username and password")]
pub async fn login(
    payload: Json<LoginPayload>,
    account_service: Data<AccountService>,
    token_service: Data<TokenService>,
) -> Result<Json<Value>, AppError> {
    if !account_service.authorize(&payload.username, &payload.password) {
        let error = json!({"error": "Incorrect username or password"});
        return Ok(Json(error));
    }
    let role = "admin";
    success_login_response(&token_service, &payload.username, role)
}

#[api_v2_operation(
    tags(Account),
    summary = "Get client type. This is used to enable/disable various interface elements.
"
)]
pub async fn client_type() -> Json<Value> {
    Json(json!({"content":"Exchange"}))
}

#[api_v2_operation(
    tags(Account),
    summary = "Get client domain. This value is used to change the image base directory on the client "
)]
pub async fn client_domain() -> Json<Value> {
    Json(json!({"content":"Domain"}))
}

#[derive(Deserialize, Apiv2Schema)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenPayload {
    refresh_token: String,
}

#[api_v2_operation(tags(Account), summary = "Refresh access-token by refresh-token")]
pub async fn refresh_token(
    token_service: Data<TokenService>,
    payload: Json<RefreshTokenPayload>,
) -> Result<Json<Value>, AppError> {
    let token = token_service.parse_refresh_token(&payload.refresh_token);
    match token {
        Ok(refresh_token) => {
            success_login_response(&token_service, &refresh_token.username, &refresh_token.role)
        }
        Err(e) => {
            log::error!("{e:?}");
            Err(AppError::Unauthorized)
        }
    }
}

fn success_login_response(
    token_service: &Data<TokenService>,
    username: &str,
    role: &str,
) -> Result<Json<Value>, AppError> {
    let new_refresh_token = token_service.generate_refresh_token(username, role);

    match new_refresh_token {
        Ok(new_refresh_token) => {
            let new_access_token = token_service.generate_access_token(username, role);
            match new_access_token {
                Ok((token, expiration)) => Ok(Json(json!({
                    "token": token,
                    "expiration": expiration,
                    "role": role,
                    "refreshToken": new_refresh_token
                }))),
                Err(e) => {
                    log::error!("{e:?}");
                    Err(AppError::Unauthorized)
                }
            }
        }
        Err(e) => {
            log::error!("{e:?}");
            Err(AppError::Unauthorized)
        }
    }
}
