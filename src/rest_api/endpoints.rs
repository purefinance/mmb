use actix_web::{error, get, post, web, Error, HttpResponse, Responder};
use log::{error, warn};
use std::sync::{mpsc::Sender, Arc};

use crate::core::{
    config::save_settings, config::CONFIG_PATH, config::CREDENTIALS_PATH,
    lifecycle::application_manager::ApplicationManager,
};

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
pub(super) async fn set_config(
    body: web::Bytes,
    application_manager: web::Data<Arc<ApplicationManager>>,
) -> Result<HttpResponse, Error> {
    let settings = std::str::from_utf8(&body)?;

    save_settings(settings, CONFIG_PATH, CREDENTIALS_PATH).map_err(|err| {
        let error_message = format!(
            "Error while trying save new config in set_config endpoint: {}",
            err.to_string()
        );
        warn!("{}", error_message);

        error::ErrorBadRequest(error_message)
    })?;

    application_manager
        .get_ref()
        .clone()
        .spawn_graceful_shutdown("Engine stopped cause config updating".to_owned());

    Ok(HttpResponse::Ok().body("Config was successfully updated. Trading engine stopped"))
}
