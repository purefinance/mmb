use actix_web::{get, HttpResponse, Responder};

// New endpoints have to be added as a servise for actix server. Look at super::control_panel::start_server()

#[get("/health")]
pub(super) async fn health() -> impl Responder {
    HttpResponse::Ok().body("Bot is working")
}

#[get("/stop")]
pub(super) async fn stop() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for bot stopping")
}

#[get("/stats")]
pub(super) async fn stats() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for getting stats")
}
