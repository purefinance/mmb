use crate::WsClientSession;
use actix_web::{web, Error, HttpRequest, Responder};
use actix_web_actors::ws::start;

pub async fn ws_client(req: HttpRequest, stream: web::Payload) -> Result<impl Responder, Error> {
    start(WsClientSession::default(), &req, stream)
}
