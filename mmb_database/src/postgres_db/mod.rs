pub mod events;
pub mod migrator;

use anyhow::{Context, Result};
use bb8_postgres::bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use std::str::FromStr;
use tokio_postgres::{Config, NoTls};

#[derive(Clone)]
pub struct PgPool(Pool<PostgresConnectionManager<NoTls>>);

pub async fn create_connections_pool(database_url: &str, max_size: u32) -> Result<PgPool> {
    // TODO enable tls
    let config = Config::from_str(database_url).context("building db connection config")?;
    let pg_mgr = PostgresConnectionManager::new(config, NoTls);
    let pool = Pool::builder()
        .min_idle(Some(1))
        .max_size(max_size)
        .build(pg_mgr)
        .await
        .context("building postgres connection pool")?;

    Ok(PgPool(pool))
}
