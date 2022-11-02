use crate::postgres_db::PgPool;

use serde::Deserialize;
use tokio_postgres::Row;

#[derive(Deserialize, Debug)]
pub struct CleanupSetting {
    pub table_name: String,
    pub column_name: String,
    pub period: String,
}

impl From<&Row> for CleanupSetting {
    fn from(row: &Row) -> Self {
        CleanupSetting {
            table_name: row.get(0),
            column_name: row.get(1),
            period: row.get(2),
        }
    }
}

pub async fn get_cleanup_settings(pool: &PgPool) -> anyhow::Result<Vec<CleanupSetting>> {
    let sql = "select table_name, column_name, period::text from cleanup_settings";
    let rows = pool
        .0
        .get()
        .await?
        .query(sql, &[])
        .await?
        .iter()
        .map(CleanupSetting::from)
        .collect();
    Ok(rows)
}

pub async fn cleanup_table(
    pool: &PgPool,
    table_name: &str,
    column_name: &str,
    period: &str,
) -> anyhow::Result<()> {
    let sql =
        format!("delete from {table_name} where {column_name} < now() - '{period}'::interval");
    pool.0.get().await?.execute(&sql, &[]).await?;
    Ok(())
}
