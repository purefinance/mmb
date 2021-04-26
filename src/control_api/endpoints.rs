use actix_web::{get, App, HttpResponse, HttpServer, Responder};

pub async fn start_control_server(address: &str) -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(health).service(stop).service(stats))
        .bind(address)?
        .run()
        .await
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("Bot is working")
}

#[get("/stop")]
async fn stop() -> impl Responder {
    HttpResponse::Ok().body("Stub for bot stoping")
}

#[get("/stats")]
async fn stats() -> impl Responder {
    HttpResponse::Ok().body("Stub for getting stats")
}
