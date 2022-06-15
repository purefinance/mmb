use crate::postgres_db::Client;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures::pin_mut;
use serde_json::Value as JsonValue;
use std::fmt::{Display, Formatter};
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::Type;

pub type TableName = &'static str;

const EVENT_INSERT_TYPES_LIST: [Type; 3] = [Type::TIMESTAMPTZ, Type::INT4, Type::JSONB];

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

#[derive(Debug)]
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
    client: &'a mut Client,
    table_name: TableName,
    events: &'a [InsertEvent],
) -> Result<()> {
    let sql = format!("COPY {table_name} (insert_time, version, json) from stdin BINARY");
    let sink = client
        .0
        .copy_in(&sql)
        .await
        .context("from `save_events_batch` on call `copy_in`")?;

    let writer = BinaryCopyInWriter::new(sink, &EVENT_INSERT_TYPES_LIST);
    pin_mut!(writer);
    let now = Utc::now();
    for event in events {
        writer
            .as_mut()
            .write(&[&now, &event.version, &event.json])
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
    client: &mut Client,
    table_name: TableName,
    events: Vec<InsertEvent>,
) -> (Result<()>, Vec<InsertEvent>) {
    let sql = format!("INSERT INTO {table_name} (insert_time, version, json) VALUES($1, $2, $3)");
    let sql_statement = match client
        .0
        .prepare_typed(&sql, &EVENT_INSERT_TYPES_LIST)
        .await
        .context("from `save_events_by_1` on client.prepare_types")
    {
        Ok(v) => v,
        Err(err) => return (Err(err), events),
    };

    let now = Utc::now();

    let mut failed_events = vec![];
    for event in events {
        let insert_result = client
            .0
            .execute(&sql_statement, &[&now, &event.version, &event.json])
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
    use crate::postgres_db::events::{save_events_batch, InsertEvent};
    use crate::postgres_db::Client;
    use serde_json::json;

    const DATABASE_URL: &str = "postgres://dev:dev@localhost/tests";
    const TABLE_NAME: &str = "persons";

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "need postgres initialized for tests"]
    async fn save_batch_events_1_item() {
        // arrange
        let client = connect().await;

        let _ = client
            .batch_execute(
                &include_str!("./sql/create_or_truncate_table.sql")
                    .replace("TABLE_NAME", TABLE_NAME),
            )
            .await
            .expect("truncate persons");

        let expected_json = json!({
            "first_name": "Иван",
            "last_name": "Иванов",
        });
        let item = InsertEvent {
            version: 1,
            json: expected_json.clone(),
        };

        let mut client = Client(client);

        // act
        save_events_batch(&mut client, TABLE_NAME, &[item])
            .await
            .expect("in test");

        // assert
        let rows = client
            .0
            .query(&format!("select * from {TABLE_NAME}"), &[])
            .await
            .expect("select persons");

        assert_eq!(rows.len(), 1);
        let row = rows.first().expect("in test");
        let version: i32 = row.get("version");
        let json: serde_json::Value = row.get("json");
        assert_eq!(version, 1);
        assert_eq!(json, expected_json);
    }

    async fn connect() -> tokio_postgres::Client {
        let (Client(client), connection) = crate::postgres_db::connect(DATABASE_URL)
            .await
            .expect("connect to db");

        let _ =
            tokio::spawn(
                async move { connection.handle().await.expect("connection error in test") },
            );
        client
    }
}
