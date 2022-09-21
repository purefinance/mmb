use actix_web::HttpResponse;
use paperclip::actix::api_v2_errors;
use thiserror::Error;

#[api_v2_errors(code = 400, code = 401, code = 500)]
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Bad request")]
    BadRequest,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal server error")]
    InternalServerError,
}

impl actix_web::error::ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::BadRequest => HttpResponse::BadRequest().finish(),
            AppError::Unauthorized => HttpResponse::Unauthorized().finish(),
            AppError::InternalServerError => HttpResponse::InternalServerError().finish(),
        }
    }
}
