use crate::postgres_db::PgPool;
use anyhow::{bail, Context, Result};
use bb8_postgres::bb8::PooledConnection;
use bb8_postgres::PostgresConnectionManager;
use chrono::{DateTime, Utc};
use futures::pin_mut;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt::{Display, Formatter};
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::Type;
use tokio_postgres::{NoTls, Statement};
pub type TableName = &'static str;
pub type TableNameRef<'a> = &'a str;

const EVENT_INSERT_TYPES_LIST: [Type; 2] = [Type::INT4, Type::JSONB];

#[macro_export]
macro_rules! impl_event {
    ($ty:ty, $table_name:expr) => {
        impl mmb_database::postgres_db::events::Event for $ty {
            fn get_table_name(&self) -> TableName {
                $table_name
            }

            fn get_json(&self) -> serde_json::Result<serde_json::Value> {
                serde_json::to_value(self)
            }
        }
    };
}

pub trait Event {
    fn get_table_name(&self) -> TableName;
    fn get_version(&self) -> i32 {
        1
    }

    fn get_json(&self) -> serde_json::Result<JsonValue>;
}

#[derive(Debug, Clone)]
pub struct DbEvent {
    pub id: u64,
    pub insert_time: DateTime<Utc>,
    pub version: i32,
    pub json: JsonValue,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct InsertEvent {
    pub version: i32,
    pub json: JsonValue,
}

impl Display for InsertEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.version, self.json)
    }
}

pub async fn save_events_batch<'a>(
    pool: &'a PgPool,
    table_name: &str,
    events: &'a [InsertEvent],
) -> Result<()> {
    let sql = format!("COPY {table_name} (version, json) from stdin BINARY");

    let sink = pool
        .0
        .get()
        .await
        .context("getting db connection from pool")?
        .copy_in(&sql)
        .await
        .context("from `save_events_batch` on call `copy_in`")?;

    let writer = BinaryCopyInWriter::new(sink, &EVENT_INSERT_TYPES_LIST);
    pin_mut!(writer);
    for event in events {
        writer
            .as_mut()
            .write(&[&event.version, &event.json])
            .await
            .context("from `save_events_batch` on CopyInWriter::write() row")?;
    }

    let added_rows_count = writer
        .finish()
        .await
        .context("from `save_events_batch` CopyInWriter::finish()")?;

    let events_count = events.len();
    if added_rows_count as usize != events_count {
        bail!("Only {added_rows_count} of {events_count} events was writen in Database");
    }

    Ok(())
}

pub async fn save_events_one_by_one(
    pool: &PgPool,
    table_name: &'_ str,
    events: Vec<InsertEvent>,
) -> (Result<()>, Vec<InsertEvent>) {
    async fn prepare_connection<'a>(
        pool: &'a PgPool,
        table_name: &'_ str,
    ) -> Result<(
        PooledConnection<'a, PostgresConnectionManager<NoTls>>,
        Statement,
    )> {
        let sql = format!("INSERT INTO {table_name} (version, json) VALUES($1, $2)");

        let connection = pool
            .0
            .get()
            .await
            .context("getting db connection from pool")?;

        let statement = connection
            .prepare_typed(&sql, &EVENT_INSERT_TYPES_LIST)
            .await
            .context("from `save_events_by_1` on client.prepare_types")?;

        Ok((connection, statement))
    }

    let (connection, sql_statement) = match prepare_connection(pool, table_name).await {
        Ok(v) => v,
        Err(err) => return (Err(err), events),
    };

    let mut failed_events = vec![];
    for event in events {
        let insert_result = connection
            .execute(&sql_statement, &[&event.version, &event.json])
            .await;

        match insert_result {
            Ok(0) => {
                log::error!(
                    "in `save_events_one_by_one` inserted 0 events, but should be 1. Event: {event}"
                );
                failed_events.push(event);
            }
            Ok(1) => { /*nothing to do*/ }
            Ok(added) => {
                log::error!("in `save_events_one_by_one` inserted {added} events, but should be 1")
            }
            Err(err) => {
                log::error!(
                    "in `save_events_one_by_one` with error {err} failed saving event: {event}"
                );

                failed_events.push(event);
            }
        }
    }

    (Ok(()), failed_events)
}

#[cfg(test)]
mod tests {
    use crate::postgres_db::events::{save_events_batch, save_events_one_by_one, InsertEvent};
    use crate::postgres_db::tests::{get_database_url, PgPoolMutex};
    use serde_json::json;

    const TABLE_NAME: &str = "persons";

    async fn init_test() -> PgPoolMutex {
        let pool_mutex = PgPoolMutex::create(&get_database_url(), 1).await;
        let connection = pool_mutex.pool.get_connection_expected().await;
        connection
            .batch_execute(
                &include_str!("./sql/create_or_truncate_table.sql")
                    .replace("TABLE_NAME", TABLE_NAME),
            )
            .await
            .expect("TRUNCATE persons");

        drop(connection);
        pool_mutex
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn save_batch_events_1_item() {
        let pool = init_test().await;

        // arrange
        let expected_json = json!({
            "first_name": "Ivan",
            "last_name": "Ivanov",
        });
        let item = InsertEvent {
            version: 1,
            json: expected_json.clone(),
        };

        // act
        save_events_batch(&pool.pool, TABLE_NAME, &[item])
            .await
            .expect("in test");

        // assert
        let connection = pool.pool.get_connection_expected().await;

        let rows = connection
            .query(&format!("SELECT * FROM {TABLE_NAME}"), &[])
            .await
            .expect("select persons");

        assert_eq!(rows.len(), 1);
        let row = rows.first().expect("in test");
        let version: i32 = row.get("version");
        let json: serde_json::Value = row.get("json");
        assert_eq!(version, 1);
        assert_eq!(json, expected_json);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn save_one_by_one_events_1_item() {
        let pool_mutex = init_test().await;

        // arrange
        let expected_json = json!({
            "first_name": "Ivan",
            "last_name": "Ivanov",
        });
        let item = InsertEvent {
            version: 1,
            json: expected_json.clone(),
        };

        // act
        let (results, failed_events) =
            save_events_one_by_one(&pool_mutex.pool, TABLE_NAME, vec![item]).await;
        results.expect("in test");

        // assert
        assert_eq!(failed_events.len(), 0, "there are failed saving events");

        let connection = pool_mutex.pool.get_connection_expected().await;

        let rows = connection
            .query(&format!("SELECT * FROM {TABLE_NAME}"), &[])
            .await
            .expect("select persons");

        assert_eq!(rows.len(), 1);
        let row = rows.first().expect("in test");
        let version: i32 = row.get("version");
        let json: serde_json::Value = row.get("json");
        assert_eq!(version, 1);
        assert_eq!(json, expected_json);
    }
}
