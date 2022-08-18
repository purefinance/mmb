mod fallback;

use crate::database::events::recorder::fallback::EventRecorderFallback;
use crate::infrastructure::spawn_future;
use anyhow::{bail, Context, Result};
use mmb_database::postgres_db::events::{
    save_events_batch, save_events_one_by_one, Event, InsertEvent, TableName,
};
use mmb_database::postgres_db::PgPool;
use mmb_utils::infrastructure::SpawnFutureFlags;
use mmb_utils::logger::print_info;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

const BATCH_MAX_SIZE: usize = 65_536;
const BATCH_SIZE_TO_SAVE: usize = 250;
const SAVING_TIMEOUT: Duration = Duration::from_secs(1);

/// timeout for checking connection health is 5 sec therefore timeout for restoring events should be significant bigger
const RESTORING_EVENTS_TIMEOUT: Duration = Duration::from_secs(30);

pub struct DbSettings {
    pub database_url: String,
    pub postponed_events_dir: Option<PathBuf>,
}

pub struct EventRecorder {
    data_tx: mpsc::Sender<(TableName, InsertEvent)>,
    shutdown_signal_tx: mpsc::UnboundedSender<()>,
    shutdown_rx: Mutex<Option<oneshot::Receiver<Result<()>>>>,
}

impl EventRecorder {
    pub async fn start(database_settings: Option<DbSettings>) -> Result<Arc<EventRecorder>> {
        let (data_tx, data_rx) = mpsc::channel(20_000);
        let (shutdown_signal_tx, shutdown_signal_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        match database_settings {
            None => {
                let _ = shutdown_tx.send(Ok(()));
                print_info(
                    "EventRecorder is not started because `database_url` is not set in settings",
                )
            }
            Some(DbSettings {
                database_url,
                postponed_events_dir,
            }) => {
                let fallback = EventRecorderFallback::new(postponed_events_dir)
                    .context("failed creation EventRecorderFallback")?;

                let pool = PgPool::create(&database_url, 5).await.with_context(|| {
                    format!("from `start_db_event_recorder` with connection_string: {database_url}")
                })?;

                let _ = spawn_future(
                    "start db event recorder",
                    SpawnFutureFlags::DENY_CANCELLATION | SpawnFutureFlags::STOP_BY_TOKEN,
                    start_db_event_recorder(
                        pool.clone(),
                        data_rx,
                        shutdown_signal_rx,
                        shutdown_tx,
                        fallback.clone(),
                    ),
                );

                let _ = spawn_future(
                    "start postponed events restoring",
                    SpawnFutureFlags::DENY_CANCELLATION | SpawnFutureFlags::STOP_BY_TOKEN,
                    start_postponed_events_restoring(pool, fallback),
                );

                print_info("EventRecorder started");
            }
        }

        Ok(Arc::new(Self {
            data_tx,
            shutdown_signal_tx,
            shutdown_rx: Mutex::new(Some(shutdown_rx)),
        }))
    }

    pub fn save<E: Event>(&self, event: E) -> Result<()> {
        if !self.data_tx.is_closed() {
            self.data_tx
                .try_send((
                    E::TABLE_NAME,
                    InsertEvent {
                        version: event.get_version(),
                        json: event
                            .get_json()
                            .context("serialization to json in `EventRecorder::save()`")?,
                    },
                ))
                .context("failed EventRecorder::save()")?
        }

        Ok(())
    }

    pub async fn flush_and_stop(&self) -> Result<()> {
        let _ = self.shutdown_signal_tx.send(());
        let receiver = self.shutdown_rx.lock().take();
        match receiver {
            None => bail!("Called method EventRecorder::flush_and_stop() with shutdown_rx=None"),
            Some(shutdown_rx) => shutdown_rx.await.unwrap_or_else(|_| {
                bail!("EventRecorder shutdown_tx dropped without sending any result")
            }),
        }
    }
}

async fn start_postponed_events_restoring(
    pool: PgPool,
    fallback: EventRecorderFallback,
) -> Result<()> {
    let mut interval = tokio::time::interval(RESTORING_EVENTS_TIMEOUT);
    loop {
        let _ = interval.tick().await;

        let mut file_names = fallback
            .get_existing_postponed_events_file_names()
            .await
            .context("can't get existing postponed events files")?;

        if file_names.is_empty() {
            // nothing to restore
            continue;
        }

        if !pool.is_connection_health().await {
            continue;
        }

        file_names.sort();
        fallback
            .try_restore_to_db_postponed_events(&pool, &file_names)
            .await;
    }
}

async fn start_db_event_recorder(
    pool: PgPool,
    mut data_rx: mpsc::Receiver<(TableName, InsertEvent)>,
    mut shutdown_signal_rx: mpsc::UnboundedReceiver<()>,
    shutdown_tx: oneshot::Sender<Result<()>>,
    fallback: EventRecorderFallback,
) -> Result<()> {
    fn create_batch_size_vec() -> Vec<InsertEvent> {
        Vec::<InsertEvent>::with_capacity(BATCH_MAX_SIZE)
    }

    #[derive(Debug)]
    struct EventsByTableName {
        events: Vec<InsertEvent>,
        last_time_to_save: Instant,
    }
    impl Default for EventsByTableName {
        fn default() -> Self {
            Self {
                events: create_batch_size_vec(),
                last_time_to_save: Instant::now(),
            }
        }
    }
    let mut events_map = HashMap::<TableName, EventsByTableName>::new();
    loop {
        let mut interval = tokio::time::interval(SAVING_TIMEOUT);
        tokio::select! {
            _ = shutdown_signal_rx.recv() => break, // in any case we should correctly finish
            result = data_rx.recv() => {
                match result {
                    Some((table_name, event)) => {
                        let EventsByTableName{ ref mut events, ref mut last_time_to_save } = events_map.entry(table_name).or_default();
                        events.push(event);

                        if last_time_to_save.elapsed() > SAVING_TIMEOUT ||
                            events.len() >= BATCH_SIZE_TO_SAVE {

                            let events = mem::replace(events, create_batch_size_vec());
                            save_batch(&pool, table_name, events, &fallback).await.context("from `start_db_event_recorder` in `save_batch`")?;

                            *last_time_to_save = Instant::now();
                        }
                    },
                    None => break, // in any case we should correctly finish
                }
            },
            _ = interval.tick() => {
                for (table_name, EventsByTableName { ref mut events, ref mut last_time_to_save }) in &mut events_map {
                    if last_time_to_save.elapsed() < SAVING_TIMEOUT {
                        let events = mem::replace(events, create_batch_size_vec());
                        save_batch(&pool, table_name, events, &fallback).await.context("from `start_db_event_recorder` in `save_batch`")?;

                        *last_time_to_save = Instant::now();
                    }
                }
            }
        }
    }

    async fn flush_all_events(
        pool: &PgPool,
        mut data_rx: mpsc::Receiver<(TableName, InsertEvent)>,
        mut events_map: HashMap<TableName, EventsByTableName>,
        fallback: EventRecorderFallback,
    ) -> Result<()> {
        while let Ok((table_name, event)) = data_rx.try_recv() {
            events_map.entry(table_name).or_default().events.push(event);
        }

        for (table_name, EventsByTableName { events, .. }) in events_map {
            save_batch(pool, table_name, events, &fallback)
                .await
                .context("from `flush_all_events` in `save_batch`")?;
        }

        Ok(())
    }

    let flush_result = flush_all_events(&pool, data_rx, events_map, fallback).await;

    let _ = shutdown_tx.send(flush_result);

    Ok(())
}

async fn save_batch(
    pool: &PgPool,
    table_name: &'_ str,
    events: Vec<InsertEvent>,
    fallback: &EventRecorderFallback,
) -> Result<()> {
    match save_events_batch(pool, table_name, &events).await {
        Ok(()) => return Ok(()),
        Err(err) => log::error!("Failed to save batch of events with error: {err:?}"),
    }

    let (saving_result, not_written_events) =
        save_events_one_by_one(pool, table_name, events).await;
    match saving_result {
        Ok(()) => if !not_written_events.is_empty() {},
        Err(err) => {
            log::error!("Failed to save events one by one with error: {err:?}");
            save_to_file(table_name, not_written_events, fallback).await;
        }
    }

    Ok(())
}

async fn save_to_file(
    table_name: &str,
    not_written_events: Vec<InsertEvent>,
    fallback: &EventRecorderFallback,
) {
    let saving_result = fallback
        .save_to_file(table_name.to_string(), not_written_events)
        .await;

    if let Err(err) = saving_result {
        log::error!("Can't save to file not written events in EventRecorderFallback: {err:?}");
    };
}

#[cfg(test)]
mod tests {
    use crate::database::events::recorder::{DbSettings, EventRecorder};
    use crate::infrastructure::init_lifetime_manager;
    use mmb_database::impl_event;
    use mmb_database::postgres_db::tests::{get_database_url, PgPoolMutex};
    use serde::{Deserialize, Serialize};
    use std::time::{Duration, Instant};
    use tokio::time::sleep;

    const TABLE_NAME: &str = "persons";

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Address {
        street_address: String,
        city: String,
        postal_code: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Person {
        first_name: String,
        last_name: String,
        address: Address,
        phone_numbers: Vec<String>,
    }

    async fn init_test() -> PgPoolMutex {
        init_lifetime_manager();

        let pool_mutex = PgPoolMutex::create(&get_database_url(), 1).await;
        let connection = pool_mutex.pool.get_connection_expected().await;
        connection
            .batch_execute(
                &include_str!(
                    "../../../../../mmb_database/src/postgres_db/sql/create_or_truncate_table.sql"
                )
                .replace("TABLE_NAME", TABLE_NAME),
            )
            .await
            .expect("TRUNCATE persons");

        drop(connection);
        pool_mutex
    }

    impl_event!(Person, TABLE_NAME);

    fn test_person() -> Person {
        Person {
            first_name: "Ivan".to_string(),
            last_name: "Ivanov".to_string(),
            address: Address {
                street_address: "Moscow st, 101".to_string(),
                city: "Petersburg".to_string(),
                postal_code: 101101,
            },
            phone_numbers: vec!["812 123-1234".to_string(), "916 123-4567".to_string()],
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn save_1_event() {
        let pool_mutex = init_test().await;
        let connection = pool_mutex.pool.get_connection_expected().await;

        let event_recorder = EventRecorder::start(Some(DbSettings {
            database_url: get_database_url(),
            postponed_events_dir: None,
        }))
        .await
        .expect("in test");

        let person = test_person();
        event_recorder.save(person).expect("in test");

        sleep(Duration::from_millis(1_500)).await;

        let rows = connection
            .query("select * from persons", &[])
            .await
            .expect("select persons in test");

        assert_eq!(rows.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn not_save_1_event_without_db_initialization() {
        let pool_mutex = init_test().await;
        let connection = pool_mutex.pool.get_connection_expected().await;

        // arrange
        let person = test_person();

        // act
        let event_recorder = EventRecorder::start(None).await.expect("in test");

        event_recorder.save(person).expect("in test");

        sleep(Duration::from_secs(2)).await;

        // assert
        let rows = connection
            .query("select * from persons", &[])
            .await
            .expect("select persons in test");

        assert_eq!(rows.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn simple_flush_and_stop() {
        let pool_mutex = init_test().await;
        let connection = pool_mutex.pool.get_connection_expected().await;

        // arrange
        let person = test_person();

        let db_settings = DbSettings {
            database_url: get_database_url(),
            postponed_events_dir: None,
        };

        // act
        let event_recorder = EventRecorder::start(Some(db_settings))
            .await
            .expect("in test");

        let timer = Instant::now();
        event_recorder.save(person).expect("in test");

        event_recorder
            .flush_and_stop()
            .await
            .expect("failed flush_and_stop in test");

        let saving_event_time = timer.elapsed();

        // assert
        assert!(
            saving_event_time < Duration::from_secs(1),
            "expected fast execution ({saving_event_time:?} < 1sec)"
        );

        let rows = connection
            .query("select * from persons", &[])
            .await
            .expect("select persons in test");

        assert_eq!(rows.len(), 1);
    }
}
