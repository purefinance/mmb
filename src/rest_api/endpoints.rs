use actix_web::{get, App, HttpResponse, HttpServer, Responder};

pub async fn start_rest_api_server(address: &str) -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(health).service(stop).service(stats))
        .bind(address)?
        .shutdown_timeout(3)
        .workers(1)
        .run()
        .await
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("Bot is working")
}

#[get("/stop")]
async fn stop() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for bot stopping")
}

#[get("/stats")]
async fn stats() -> impl Responder {
    // TODO It is just a stub. Fix method body in the future
    HttpResponse::Ok().body("Stub for getting stats")
}
