use super::control_panel::ControlPanel;
use actix_web::{get, post, web, HttpResponse, Responder};
use std::sync::{mpsc::Sender, Arc};

// New endpoints have to be added as a service for actix server. Look at super::control_panel::start_server()

#[get("/health")]
pub(super) async fn health() -> impl Responder {
    dbg!(&"HEALTH");
    HttpResponse::Ok().body("Bot is working")
}

#[post("/stop")]
pub(super) async fn stop(server_stopper_tx: web::Data<Sender<()>>) -> impl Responder {
    let stop_receiver = control_panel.get_ref().clone().stop();
    let outcome = stop_receiver.await;
    dbg!(&"STOP AWAITED");
    // TODO It is just a stub. Fix method body in the future
    //HttpResponse::Ok().body("Stub for bot stopping")
    HttpResponse::NoContent().finish()
}

#[get("/stats")]
pub(super) async fn stats() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for getting stats")
}
