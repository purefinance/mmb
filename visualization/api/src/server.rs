use crate::config::Market;
use crate::data_provider::DataProvider;
use crate::middleware::auth::TokenAuth;
use crate::routes::routes;
use crate::services::account::AccountService;
use crate::services::auth::AuthService;
use crate::services::data_provider::balances::BalancesService;
use crate::services::market_settings::MarketSettingsService;
use crate::services::settings::SettingsService;
use crate::services::token::TokenService;
use crate::ws::actors::error_listener::ErrorListener;
use crate::ws::actors::new_data_listener::NewDataListener;
use crate::ws::actors::subscription_manager::SubscriptionManager;
use crate::LiquidityService;
use actix::{spawn, Actor};
use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{App, HttpServer};
use casbin::Enforcer;
use sqlx::postgres::PgPoolOptions;
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

    let liquidity_service = LiquidityService::new(connection_pool.clone());
    let balances_service = BalancesService::new(connection_pool.clone());
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
    let settings_service = Arc::new(SettingsService::new(connection_pool));

    let data_provider = DataProvider::new(
        subscription_manager,
        liquidity_service,
        market_settings_service.clone(),
        new_data_listener,
        error_listener,
        balances_service,
    );

    spawn(async move {
        let mut interval = time::interval(Duration::from_millis(refresh_data_interval_ms));

        loop {
            if let Err(e) = data_provider.step().await {
                log::error!("Failure step data provider {e}")
            };
            interval.tick().await;
        }
    });

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
            .app_data(Data::new(settings_service.clone()))
    })
    .bind(address)?
    .run()
    .await
}
