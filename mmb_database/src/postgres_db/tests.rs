use crate::postgres_db::PgPool;
use once_cell::sync::Lazy;
use parking_lot::{ReentrantMutex, ReentrantMutexGuard};
use std::env;

pub static MUTEX: Lazy<ReentrantMutex<()>> = Lazy::new(ReentrantMutex::default);
const DATABASE_URL: &str = "postgres://postgres:postgres@localhost/tests";

pub struct PgPoolMutex {
    pub pool: PgPool,
    pub mutex: ReentrantMutexGuard<'static, ()>,
}

impl PgPoolMutex {
    pub async fn create(database_url: &str, size: u32) -> PgPoolMutex {
        let pool = PgPool::create(database_url, size)
            .await
            .expect("connect to db");
        Self {
            pool,
            mutex: MUTEX.lock(),
        }
    }
}

pub fn get_database_url() -> String {
    env::var("DATABASE_URL_TEST").unwrap_or_else(|_| DATABASE_URL.to_string())
}
