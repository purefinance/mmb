use crate::config::Market;
use crate::middleware::auth::TokenAuth;
use crate::routes::routes;
use crate::services::account::AccountService;
use crate::services::auth::AuthService;
use crate::services::market_settings::MarketSettingsService;
use crate::services::token::TokenService;
use crate::ws::actors::error_listener::ErrorListener;
use crate::ws::actors::new_data_listener::NewDataListener;
use crate::ws::actors::subscription_manager::SubscriptionManager;
use crate::ws::broker_messages::{
    ClearSubscriptions, GatherSubscriptions, GetLiquiditySubscriptions, SubscriptionErrorMessage,
};
use crate::ws::subscribes::liquidity::{LiquiditySubscription, Subscription};
use crate::{LiquidityService, NewLiquidityDataMessage};
use actix::{spawn, Actor, Addr};
use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use casbin::Enforcer;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

#[allow(clippy::too_many_arguments)]
pub async fn start(
    address: &str,
    access_token_secret: String,
    refresh_token_secret: String,
    access_token_lifetime: i64,
    refresh_token_lifetime: i64,
    database_url: &str,
    enforcer: Enforcer,
    markets: Vec<Market>,
    refresh_data_interval_ms: u64,
) -> std::io::Result<()> {
    log::info!("Starting server at {address}");
    let connection_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Unable to connect to DB");

    let liquidity_service = LiquidityService::new(connection_pool);
    let new_data_listener = NewDataListener::default().start();
    let error_listener = ErrorListener::default().start();
    let account_service = AccountService::default();
    let token_service = TokenService::new(
        access_token_secret,
        refresh_token_secret,
        access_token_lifetime,
        refresh_token_lifetime,
    );
    let subscription_manager = SubscriptionManager::default().start();
    let auth_service = Arc::new(AuthService::new(enforcer));
    let market_settings_service = Arc::new(MarketSettingsService::from(markets));

    spawn(data_provider(
        subscription_manager,
        liquidity_service,
        market_settings_service.clone(),
        new_data_listener,
        error_listener,
        refresh_data_interval_ms,
    ));

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .configure(routes)
            .wrap(cors)
            .wrap(Logger::default())
            .wrap(TokenAuth::default())
            .app_data(Data::new(account_service.clone()))
            .app_data(Data::new(auth_service.clone()))
            .app_data(Data::new(token_service.clone()))
            .app_data(Data::new(market_settings_service.clone()))
    })
    .workers(2)
    .bind(address)?
    .run()
    .await
}

async fn data_provider(
    subscription_manager: Addr<SubscriptionManager>,
    liquidity_service: LiquidityService,
    market_settings_service: Arc<MarketSettingsService>,
    new_data_listener: Addr<NewDataListener>,
    error_listener: Addr<ErrorListener>,
    refresh_data_interval_ms: u64,
) {
    let mut interval = time::interval(Duration::from_millis(refresh_data_interval_ms));
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

        for sub in subscriptions {
            let liquidity_data = liquidity_service
                .get_liquidity_data(&sub.exchange_id, &sub.currency_pair, 20)
                .await;

            match liquidity_data {
                Ok(mut liquidity_data) => {
                    let desired_amount = market_settings_service
                        .get_desired_amount(&sub.exchange_id, &sub.currency_pair);

                    match desired_amount {
                        None => {
                            log::error!(
                                "Desired amount is none for {} {}",
                                &sub.exchange_id,
                                &sub.currency_pair
                            );
                            let message = SubscriptionErrorMessage {
                                subscription: sub.get_hash(),
                                message: "Bad request".to_string(),
                            };
                            let _ = error_listener.send(message).await;
                        }
                        Some(desired_amount) => {
                            liquidity_data.desired_amount = desired_amount;
                            let _ = new_data_listener
                                .send(NewLiquidityDataMessage {
                                    subscription: sub,
                                    data: liquidity_data,
                                })
                                .await;
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failure to load liquidity data from database. Filters: {sub:?}. Error: {e:?}");
                    let message = SubscriptionErrorMessage {
                        subscription: sub.get_hash(),
                        message: "Internal server error".to_string(),
                    };
                    let _ = error_listener.send(message).await;
                }
            }
        }
        interval.tick().await;
    }
}
