use crate::middleware::auth::TokenAuth;
use crate::routes::routes;
use crate::services::account::AccountService;
use crate::services::auth::AuthService;
use crate::services::token::TokenService;
use crate::ws::actors::new_data_listener::NewDataListener;
use crate::ws::actors::subscription_manager::SubscriptionManager;
use crate::ws::broker_messages::{
    ClearSubscriptions, GatherSubscriptions, GetLiquiditySubscriptions,
};
use crate::ws::subscribes::liquidity::LiquiditySubscription;
use crate::{LiquidityService, NewLiquidityDataMessage};
use actix::clock::sleep;
use actix::{spawn, Actor};
use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use casbin::Enforcer;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

pub async fn start(
    port: u16,
    access_token_secret: String,
    refresh_token_secret: String,
    access_token_lifetime: i64,
    refresh_token_lifetime: i64,
    database_url: &str,
    enforcer: Enforcer,
) -> std::io::Result<()> {
    log::info!("Starting server at 127.0.0.1:{}", port);
    let connection_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Unable to connect to DB");
    let liquidity_service = LiquidityService::new(connection_pool);
    let new_data_listener = NewDataListener::default().start();
    let account_service = AccountService::default();
    let token_service = TokenService::new(
        access_token_secret,
        refresh_token_secret,
        access_token_lifetime,
        refresh_token_lifetime,
    );
    let subscription_manager = SubscriptionManager::default().start();
    let auth_service = Arc::new(AuthService::new(enforcer));

    spawn(async move {
        loop {
            subscription_manager
                .send(ClearSubscriptions)
                .await
                .expect("Failure to execute subscription manager");
            subscription_manager
                .send(GatherSubscriptions)
                .await
                .expect("Failure to execute subscription manager");
            let subscriptions: HashSet<LiquiditySubscription> = subscription_manager
                .send(GetLiquiditySubscriptions)
                .await
                .expect("Failure to execute subscription manager");
            let liquidity_array = liquidity_service
                .get_liquidity_data_by_subscriptions(&subscriptions)
                .await;

            match liquidity_array {
                Ok(liquidity_array) => {
                    for liquidity_data in liquidity_array {
                        let _ = new_data_listener
                            .send(NewLiquidityDataMessage {
                                data: liquidity_data,
                            })
                            .await;
                    }
                }
                Err(e) => {
                    log::error!("Failure to load liquidity data from database. Filters: {subscriptions:?}. Error: {e:?}")
                }
            }
            sleep(Duration::from_millis(1000)).await;
        }
    });

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .configure(routes)
            .wrap(cors)
            .wrap(Logger::default())
            .wrap(TokenAuth::default())
            .app_data(web::Data::new(account_service.clone()))
            .app_data(web::Data::new(auth_service.clone()))
            .app_data(web::Data::new(token_service.clone()))
    })
    .workers(2)
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
