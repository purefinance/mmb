use actix_web::{get, post, web, HttpResponse, Responder};
use futures::FutureExt;

use crate::control_panel::{send_request, DataWebMmbRpcClient};

// New endpoints have to be added as a service for actix server and webui control page. Look at super::control_panel::start() and webui/README.md

#[get("/health")]
pub(super) async fn health(client: DataWebMmbRpcClient) -> impl Responder {
    send_request(client, |client| client.health().boxed()).await
}

#[post("/stop")]
pub(super) async fn stop(client: DataWebMmbRpcClient) -> impl Responder {
    send_request(client, |client| client.stop().boxed()).await
}

#[get("/config")]
pub(super) async fn get_config(client: DataWebMmbRpcClient) -> impl Responder {
    send_request(client, |client| client.get_config().boxed()).await
}

#[post("/config")]
pub(super) async fn set_config(body: web::Bytes, client: DataWebMmbRpcClient) -> impl Responder {
    let settings = match String::from_utf8((&body).to_vec()) {
        Ok(settings) => settings,
        Err(err) => {
            return HttpResponse::BadRequest().body(format!(
                "Failed to convert input settings({body:?}) to utf8 string: {err}",
            ))
        }
    };

    send_request(client, move |client| {
        client.set_config(settings.clone()).boxed()
    })
    .await
}

#[get("/stats")]
pub(super) async fn stats(client: DataWebMmbRpcClient) -> impl Responder {
    send_request(client, |client| client.stats().boxed()).await
}
