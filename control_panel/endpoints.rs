use actix_web::{get, post, web, HttpResponse, Responder};
use futures::Future;
use jsonrpc_core::{Params, Value};
use jsonrpc_core_client::RpcError;
use shared::rest_api::gen_client;
use std::sync::{mpsc::Sender, Arc};

// New endpoints have to be added as a service for actix server. Look at super::control_panel::start_server()

#[get("/health")]
pub(super) async fn health(client: web::Data<Arc<gen_client::Client>>) -> impl Responder {
    send_request(client.health()).await
}

#[post("/stop")]
pub(super) async fn stop(
    server_stopper_tx: web::Data<Sender<()>>,
    client: web::Data<Arc<gen_client::Client>>,
) -> impl Responder {
    if let Err(error) = server_stopper_tx.send(()) {
        let err_message = format!("Unable to send signal to stop actix server: {}", error);
        log::error!("{}", err_message);
        return HttpResponse::InternalServerError().body(err_message);
    }

    send_request(client.stop()).await
}

#[get("/config")]
pub(super) async fn get_config(client: web::Data<Arc<gen_client::Client>>) -> impl Responder {
    send_request(client.get_config()).await
}

#[post("/config")]
pub(super) async fn set_config(
    body: web::Bytes,
    client: web::Data<Arc<gen_client::Client>>,
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

    let settings = Params::Array(vec![Value::String(settings)]);

    send_request(client.set_config(settings)).await
}

#[get("/stats")]
pub(super) async fn stats(client: web::Data<Arc<gen_client::Client>>) -> impl Responder {
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

async fn send_request(request: impl Future<Output = Result<Value, RpcError>>) -> HttpResponse {
    match request.await {
        Ok(response) => HttpResponse::Ok().body(response.to_string()),
        Err(err) => handle_rpc_error(err),
    }
}
