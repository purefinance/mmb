use crate::services::account::AccountService;
use crate::services::token::TokenService;
use actix_web::web::Data;
use actix_web::{get, post, web, Error, HttpResponse};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct LoginPayload {
    username: String,
    password: String,
}

#[post("/login")]
pub async fn login(
    payload: web::Json<LoginPayload>,
    account_service: Data<AccountService>,
    token_service: Data<TokenService>,
) -> Result<HttpResponse, Error> {
    if !account_service.authorize(&payload.username, &payload.password) {
        let error = json!({"error": "Incorrect username or password"});
        return Ok(HttpResponse::Ok().json(error));
    }
    let role = "admin";
    success_login_response(&token_service, &payload.username, role)
}

#[get("/clienttype")]
pub async fn client_type() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json("Exchange"))
}

#[get("/clientdomain")]
pub async fn client_domain() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json("Domain"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenPayload {
    refresh_token: String,
}

#[post("/refresh-token")]
pub async fn refresh_token(
    token_service: Data<TokenService>,
    payload: web::Json<RefreshTokenPayload>,
) -> Result<HttpResponse, Error> {
    let token = token_service.parse_refresh_token(&payload.refresh_token);
    match token {
        Ok(refresh_token) => {
            success_login_response(&token_service, &refresh_token.username, &refresh_token.role)
        }
        Err(_) => Ok(HttpResponse::Unauthorized().finish()),
    }
}

fn success_login_response(
    token_service: &Data<TokenService>,
    username: &str,
    role: &str,
) -> Result<HttpResponse, Error> {
    let new_refresh_token = token_service.generate_refresh_token(username, role);

    match new_refresh_token {
        Ok(new_refresh_token) => {
            let new_access_token = token_service.generate_access_token(username, role);
            match new_access_token {
                Ok((token, expiration)) => Ok(HttpResponse::Ok().json(json!({
                    "token": token,
                    "expiration": expiration,
                    "role": role,
                    "refreshToken": new_refresh_token
                }))),
                Err(e) => {
                    log::error!("{e:?}");
                    Ok(HttpResponse::InternalServerError().finish())
                }
            }
        }
        Err(e) => {
            log::error!("{e:?}");
            Ok(HttpResponse::InternalServerError().finish())
        }
    }
}
