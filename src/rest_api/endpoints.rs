use actix_web::{get, post, web, Error, HttpMessage, HttpRequest, HttpResponse, Responder};
use futures::stream::StreamExt;
use log::error;
use std::sync::mpsc::Sender;

use crate::core::config::update_settings;

// New endpoints have to be added as a service for actix server. Look at super::control_panel::start_server()

#[get("/health")]
pub(super) async fn health() -> impl Responder {
    HttpResponse::Ok().body("Bot is working")
}

#[post("/stop")]
pub(super) async fn stop(server_stopper_tx: web::Data<Sender<()>>) -> impl Responder {
    if let Err(error) = server_stopper_tx.send(()) {
        error!("Unable to send signal to stop actix server: {}", error);
    }

    HttpResponse::Ok().body("ControlPanel turned off")
}

#[get("/stats")]
pub(super) async fn stats() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for getting stats")
}

#[get("/config")]
pub(super) async fn get_config(engine_settings: web::Data<String>) -> impl Responder {
    HttpResponse::Ok().body(engine_settings.get_ref())
}

#[post("/config")]
pub(super) async fn set_config(body: web::Bytes) -> Result<HttpResponse, Error> {
    let settings = std::str::from_utf8(&body)?;

    let config_path = "updated_config.toml";
    let credentials_path = "updated_credentials.toml";
    // FIXME unwrap
    update_settings(settings, config_path, credentials_path).unwrap();

    // FIXME stop application via application_manager

    Ok(HttpResponse::Ok().body("Config was successfully updated. Trading engine stopped"))
}
