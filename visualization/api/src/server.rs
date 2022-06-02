use crate::routes::routes;
use crate::services::account::AccountService;
use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};

pub async fn start(
    port: u16,
    secret: String,
    access_token_lifetime_ms: i64,
) -> std::io::Result<()> {
    log::info!("Starting server at 127.0.0.1:{}", port);

    HttpServer::new(move || {
        let cors = Cors::permissive();
        let account_service = AccountService::new(secret.clone(), access_token_lifetime_ms);
        App::new()
            .configure(routes)
            .wrap(cors)
            .wrap(Logger::default())
            .app_data(web::Data::new(account_service))
    })
    .workers(2)
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
