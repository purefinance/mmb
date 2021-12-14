use actix_web::{get, post, web, HttpResponse, Responder};
use futures::Future;
use jsonrpc_core_client::RpcError;
use mmb_rpc::rest_api::MmbRpcClient;
use std::sync::Arc;

// New endpoints have to be added as a service for actix server. Look at super::control_panel::start_server()

#[get("/health")]
pub(super) async fn health(client: web::Data<Arc<MmbRpcClient>>) -> impl Responder {
    send_request(client.health()).await
}

#[post("/stop")]
pub(super) async fn stop(client: web::Data<Arc<MmbRpcClient>>) -> impl Responder {
    send_request(client.stop()).await
}

#[get("/config")]
pub(super) async fn get_config(client: web::Data<Arc<MmbRpcClient>>) -> impl Responder {
    send_request(client.get_config()).await
}

#[post("/config")]
pub(super) async fn set_config(
    body: web::Bytes,
    client: web::Data<Arc<MmbRpcClient>>,
) -> impl Responder {
    let settings = match String::from_utf8((&body).to_vec()) {
        Ok(settings) => settings,
        Err(err) => {
            return HttpResponse::BadRequest().body(format!(
                "Failed to convert input settings({:?}) to str: {}",
                body,
                err.to_string(),
            ))
        }
    };

    send_request(client.set_config(settings)).await
}

#[get("/stats")]
pub(super) async fn stats(client: web::Data<Arc<MmbRpcClient>>) -> impl Responder {
    send_request(client.stats()).await
}

fn handle_rpc_error(error: RpcError) -> HttpResponse {
    match error {
        RpcError::JsonRpcError(error) => {
            HttpResponse::InternalServerError().body(error.to_string())
        }
        RpcError::ParseError(msg, error) => HttpResponse::BadRequest().body(format!(
            "Failed to parse '{}': {}",
            msg,
            error.to_string()
        )),
        RpcError::Timeout => HttpResponse::RequestTimeout().body("Request Timeout"),
        RpcError::Client(msg) => HttpResponse::InternalServerError().body(msg),
        RpcError::Other(error) => HttpResponse::InternalServerError().body(error.to_string()),
    }
}

async fn send_request(request: impl Future<Output = Result<String, RpcError>>) -> HttpResponse {
    match request.await {
        Ok(response) => HttpResponse::Ok().body(response),
        Err(err) => handle_rpc_error(err),
    }
}
