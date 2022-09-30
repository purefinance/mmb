use crate::postgres_db::PgPool;

pub async fn save_live_range_to_db(
    pool: &PgPool,
    session_id: &str,
) -> Result<u64, tokio_postgres::Error> {
    let sql = "INSERT INTO bot_sessions(id) values ($1)
                         ON CONFLICT (id) DO UPDATE
                         SET datetime_to = now()";

    pool.0
        .get()
        .await
        .expect("Failed to get connection")
        .execute(sql, &[&session_id])
        .await
}
