use actix_web::Error;
use paperclip::actix::web::{get, post, put};
use paperclip::actix::{api_v2_operation, web, NoContent};

use crate::{handlers, ws_client};

pub(crate) fn ws_routes(app: &mut actix_web::web::ServiceConfig) {
    app.service(actix_web::web::resource("/hub/").to(ws_client));
}

#[api_v2_operation(tags(Common), summary = "Check API health status. `Ok` is 204 code")]
async fn health() -> Result<NoContent, Error> {
    Ok(NoContent)
}

pub(crate) fn http_routes(app: &mut web::ServiceConfig) {
    app.route("/health", get().to(health));
    app.service(
        web::scope("/api")
            .service(
                web::scope("/account")
                    .route("/login", post().to(handlers::account::login))
                    .route("/clienttype", get().to(handlers::account::client_type))
                    .route("/clientdomain", get().to(handlers::account::client_domain))
                    .route(
                        "/refresh-token",
                        post().to(handlers::account::refresh_token),
                    ),
            )
            .service(
                web::scope("/configuration")
                    .route("", get().to(handlers::configuration::get))
                    .route("", put().to(handlers::configuration::save))
                    .route("/validate", post().to(handlers::configuration::validate)),
            )
            .route("/explanations", get().to(handlers::explanation::get))
            .service(web::scope("/liquidity").route(
                "/supported-exchanges",
                get().to(handlers::liquidity::supported_exchanges),
            )),
    );
}
