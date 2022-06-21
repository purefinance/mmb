use crate::services::token::TokenService;
use crate::ws::actors::ws_client_session::WsClientSession;
use actix_web::{web, Error, HttpRequest, Responder};
use actix_web_actors::ws::start;

pub async fn ws_client(
    req: HttpRequest,
    stream: web::Payload,
    token_service: web::Data<TokenService>,
) -> Result<impl Responder, Error> {
    start(WsClientSession::new(token_service), &req, stream)
}
