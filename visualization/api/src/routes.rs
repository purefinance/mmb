use crate::handlers;
use crate::handlers::account::{client_domain, client_type, login, refresh_token};
use crate::handlers::liquidity::supported_exchanges;
use crate::ws_client;
use actix_web::web;
use actix_web::web::ServiceConfig;

pub(crate) fn routes(app: &mut ServiceConfig) {
    // ws
    app.service(web::resource("/hub/").to(ws_client));

    // rest
    app.service(
        web::scope("/api")
            .service(
                web::scope("/account")
                    .service(login)
                    .service(client_type)
                    .service(client_domain)
                    .service(refresh_token),
            )
            .service(web::scope("/explanations").service(handlers::explanation::get))
            .service(web::scope("/liquidity").service(supported_exchanges))
            .service(
                web::scope("/configuration")
                    .service(handlers::configuration::get)
                    .service(handlers::configuration::save)
                    .service(handlers::configuration::validate),
            ),
    );
}
