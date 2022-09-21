use std::sync::Arc;

use actix::fut::{ready, Ready};
use actix_web::dev::{forward_ready, Service, Transform};
use actix_web::error::{ErrorBadRequest, ErrorForbidden, ErrorInternalServerError};
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    web::Data,
    Error,
};
use casbin::CoreApi;
use futures::future::LocalBoxFuture;
use futures::FutureExt;

use crate::services::account::User;
use crate::services::auth::AuthService;
use crate::services::token::TokenService;

#[derive(Default)]
pub struct TokenAuth;

impl<S, B> Transform<S, ServiceRequest> for TokenAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = TokenAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TokenAuthMiddleware { service }))
    }
}

pub struct TokenAuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TokenAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let auth_service = req
            .app_data::<Data<Arc<AuthService>>>()
            .expect("Failure to get AuthService");
        let token_service = req
            .app_data::<Data<TokenService>>()
            .expect("Failure to get TokenService");

        let auth_header = req.headers().get("Authorization");
        let user = match auth_header {
            Some(auth_header) => {
                let auth_header = auth_header.to_str().unwrap_or("");
                if !auth_header.starts_with("bearer") && !auth_header.starts_with("Bearer") {
                    return async { Err(ErrorBadRequest("")) }.boxed_local();
                }
                let raw_token = auth_header[6..auth_header.len()].trim();
                let token_claim = token_service.parse_access_token(raw_token);
                match token_claim {
                    Ok(token_claim) => User::from(token_claim),
                    Err(_) => User::build_guest(),
                }
            }
            _ => User::build_guest(),
        };

        let is_auth =
            auth_service
                .enforcer
                .enforce((&user.role, &req.path(), req.method().as_str()));

        match is_auth {
            Ok(true) => self.service.call(req).boxed_local(),
            Ok(false) => async { Err(ErrorForbidden("")) }.boxed_local(),
            Err(err) => {
                log::error!("Failure to execute enforcer Error: {err:?}. Request: {req:?}");
                async { Err(ErrorInternalServerError("")) }.boxed_local()
            }
        }
    }
}
