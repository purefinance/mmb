use crate::services::account::AccountService;
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
    account_service: web::Data<AccountService>,
) -> Result<HttpResponse, Error> {
    if !account_service.authorize(&payload.username, &payload.password) {
        return Ok(HttpResponse::Unauthorized().finish());
    }
    let role = "user";
    match account_service.generate_access_token(&payload.username, role) {
        Ok((token, expiration)) => Ok(HttpResponse::Ok().json(json!({"token": token,
        "expiration": expiration,
        "role": role
        }))),
        Err(e) => {
            log::error!("{:?}", e);
            Ok(HttpResponse::InternalServerError().finish())
        }
    }
}

#[get("/clienttype")]
pub async fn client_type() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json("Exchange"))
}

#[get("/clientdomain")]
pub async fn client_domain() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json("Domain"))
}
