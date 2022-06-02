#![deny(
    non_ascii_idents,
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_must_use,
    clippy::unwrap_used
)]

mod handlers;
mod routes;
mod server;
mod services;
mod ws;

use crate::handlers::ws::ws_client;
use crate::server::start;
use crate::services::liquidity::LiquidityService;
use crate::ws::broker_messages::LiquidityDataMessage;
use crate::ws::ws_client_session::WsClientSession;
use actix::clock::sleep;
use actix::{spawn, Actor};
use std::time::Duration;
use ws::new_data_listener::NewDataListener;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Test data sender to opened sockets
    spawn(async move {
        let new_data_listener = NewDataListener::start_default();
        let liquidity_service = LiquidityService::default();
        loop {
            let data = liquidity_service.get_random_liquidity_data();
            let message = LiquidityDataMessage { data };
            match new_data_listener.try_send(message) {
                Ok(_) => {}
                Err(error) => {
                    log::error!("Failed to send message to actor {:?}", error)
                }
            }
            sleep(Duration::new(1, 0)).await;
        }
    });

    start(53938, "somesecretkey".to_string(), 31556952000).await
}
