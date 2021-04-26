use actix_web::{get, App, HttpResponse, HttpServer, Responder};

pub async fn start_control_server(address: &str) -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(health))
        .bind(address)?
        .run()
        .await
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("Bot is working")
}
