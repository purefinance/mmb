use crate::lifecycle::trading_engine::Service;
use mmb_database::postgres_db::live_ranges::save_live_range_to_db;
use mmb_database::postgres_db::PgPool;
use mmb_utils::infrastructure::WithExpect;
use std::sync::Arc;
use tokio::sync::oneshot::Receiver;

pub struct LiveRangesService {
    pool: PgPool,
    session_id: String,
}

impl Service for LiveRangesService {
    fn name(&self) -> &str {
        "LiveRangesService"
    }

    fn graceful_shutdown(self: Arc<Self>) -> Option<Receiver<anyhow::Result<()>>> {
        None
    }
}

impl LiveRangesService {
    pub fn new(session_id: String, pool: PgPool) -> Self {
        Self { pool, session_id }
    }

    pub async fn push(self: Arc<Self>) {
        let _ = save_live_range_to_db(&self.pool, &self.session_id)
            .await
            .with_expect(|| format!("Failed push live range. Session ID: {}", &self.session_id));
    }
}
