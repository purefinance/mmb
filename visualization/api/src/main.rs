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
mod middleware;
mod routes;
mod server;
mod services;
mod ws;
use crate::handlers::ws::ws_client;
use crate::server::start;
use crate::services::liquidity::LiquidityService;
use crate::ws::broker_messages::NewLiquidityDataMessage;
use casbin::{CoreApi, Enforcer};
use std::env;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let enforcer = Enforcer::new("policy/model.conf", "policy/policy.csv")
        .await
        .expect("Failure to load enforcer policy");

    start(
        53938,
        "somesecretkey1".to_string(),
        "somesecretkey2".to_string(),
        86400000,    // one day
        31556952000, // one year
        &database_url,
        enforcer,
    )
    .await
}
