pub mod events;
pub mod migrator;
pub mod tests;

use anyhow::{Context, Result};
use bb8_postgres::bb8::{Pool, PooledConnection};
use bb8_postgres::PostgresConnectionManager;
use std::str::FromStr;
use std::time::Duration;
use tokio_postgres::{Config, NoTls};

#[derive(Clone)]
pub struct PgPool(Pool<PostgresConnectionManager<NoTls>>);

impl PgPool {
    pub async fn create(database_url: &str, max_size: u32) -> Result<PgPool> {
        // TODO enable tls
        let config = Config::from_str(database_url).context("building db connection config")?;
        let pg_mgr = PostgresConnectionManager::new(config, NoTls);
        let pool = Pool::builder()
            .connection_timeout(Duration::from_secs(5))
            .min_idle(Some(1))
            .max_size(max_size)
            .build(pg_mgr)
            .await
            .context("building postgres connection pool")?;

        Ok(PgPool(pool))
    }

    // compilation error without explicit lifetimes
    #[allow(clippy::needless_lifetimes)]
    pub async fn get_connection_expected<'a>(
        &'a self,
    ) -> PooledConnection<'a, PostgresConnectionManager<NoTls>> {
        self.0.get().await.expect("getting db connection from pool")
    }

    pub async fn is_connection_health(&self) -> bool {
        self.0.get().await.is_ok()
    }
}
