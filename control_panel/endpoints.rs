use actix_web::{get, post, web, HttpResponse, Responder};
use jsonrpc_core_client::RpcError;
use mmb_rpc::rest_api::MmbRpcClient;
use parking_lot::Mutex;
use std::{sync::Arc, time::Duration};

use crate::control_panel::ControlPanel;

type WebMmbRpcClient = web::Data<Arc<Mutex<Option<MmbRpcClient>>>>;

#[derive(Clone, Copy)]
enum Request {
    Health,
    Stop,
    GetConfig,
    SetConfig,
    Stats,
}

// New endpoints have to be added as a service for actix server. Look at super::control_panel::start()

#[get("/health")]
pub(super) async fn health(client: WebMmbRpcClient) -> impl Responder {
    send_request(client, Request::Health).await
}

#[post("/stop")]
pub(super) async fn stop(client: WebMmbRpcClient) -> impl Responder {
    send_request(client, Request::Stop).await
}

#[get("/config")]
pub(super) async fn get_config(client: WebMmbRpcClient) -> impl Responder {
    send_request(client, Request::GetConfig).await
}

#[post("/config")]
pub(super) async fn set_config(body: web::Bytes, client: WebMmbRpcClient) -> impl Responder {
    let settings = match String::from_utf8((&body).to_vec()) {
        Ok(settings) => Some(settings),
        Err(err) => {
            return HttpResponse::BadRequest().body(format!(
                "Failed to convert input settings({:?}) to utf8 string: {}",
                body,
                err.to_string(),
            ))
        }
    };

    send_request_with_params(client, Request::SetConfig, settings).await
}

#[get("/stats")]
pub(super) async fn stats(client: WebMmbRpcClient) -> impl Responder {
    send_request(client, Request::Stats).await
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

async fn send_request_core(
    client: &MmbRpcClient,
    request: Request,
    params: Option<String>,
) -> Result<String, RpcError> {
    match request {
        Request::Health => client.health().await,
        Request::Stop => client.stop().await,
        Request::GetConfig => client.get_config().await,
        Request::SetConfig => {
            client
                .set_config(params.expect("Params shouldn't be None"))
                .await
        }
        Request::Stats => client.stats().await,
    }
}

async fn send_request(client: WebMmbRpcClient, request: Request) -> HttpResponse {
    send_request_with_params(client, request, None).await
}

async fn send_request_with_params(
    client: WebMmbRpcClient,
    request: Request,
    params: Option<String>,
) -> HttpResponse {
    let mut try_counter = 1;

    async fn try_reconnect(client: WebMmbRpcClient, try_counter: i32) {
        log::warn!(
            "Failed to send request {}, trying to reconnect...",
            try_counter
        );
        *client.lock() = ControlPanel::build_rpc_client().await;
    }

    loop {
        log::info!("Trying to send request attempt {}...", try_counter);

        if let Some(client) = &*client.lock() {
            match send_request_core(client, request, params.clone()).await {
                Ok(response) => return HttpResponse::Ok().body(response.to_string()),
                Err(err) => {
                    return handle_rpc_error(err);
                }
            }
        }

        try_reconnect(client.clone(), try_counter).await;

        if try_counter > 2 {
            return HttpResponse::RequestTimeout().body("Request Timeout");
        }

        std::thread::sleep(Duration::from_secs(3));
        try_counter += 1;
    }
}
