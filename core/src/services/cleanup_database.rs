use std::sync::Arc;

use tokio::sync::oneshot::Receiver;

use mmb_database::postgres_db::cleanup_database::{cleanup_table, get_cleanup_settings};
use mmb_database::postgres_db::PgPool;

use crate::lifecycle::trading_engine::Service;

pub struct CleanupDatabaseService {
    pool: PgPool,
}

impl Service for CleanupDatabaseService {
    fn name(&self) -> &str {
        "CleanupDatabaseService"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<Receiver<anyhow::Result<()>>> {
        None
    }
}

impl CleanupDatabaseService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn run(self: Arc<Self>) {
        match self.run_cleanup().await {
            Ok(()) => (),
            Err(e) => {
                log::error!("Failed to run cleanup database service. {e:?}");
            }
        }
    }

    async fn run_cleanup(&self) -> anyhow::Result<()> {
        let settings = get_cleanup_settings(&self.pool).await?;

        for setting in settings {
            cleanup_table(
                &self.pool,
                &setting.table_name,
                &setting.column_name,
                &setting.period,
            )
            .await?;
        }
        Ok(())
    }
}
