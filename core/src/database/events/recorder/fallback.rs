use crate::database::events::recorder::save_batch;
use crate::exchanges::timeouts::timeout_manager;
use anyhow::{Context, Result};
use itertools::Itertools;
use mmb_database::postgres_db::events::InsertEvent;
use mmb_database::postgres_db::PgPool;
use mmb_utils::{nothing_to_do, DateTime};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::{create_dir_all, DirEntry, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs};
use tokio::task::spawn_blocking;

const BUFFER_SIZE: usize = 16384;
const EVENTS_FILE_PREFIX: &str = "events_";
const NOT_FINISHED_FILED_PREFIX: &str = "writing_yet_";

fn get_postponed_events_dir(
    postponed_events_dir_from_settings: Option<PathBuf>,
) -> Result<Arc<Path>> {
    match postponed_events_dir_from_settings {
        Some(dir) => return Ok(dir.as_path().into()),
        None => nothing_to_do(),
    }

    const POSTPONED_EVENTS_FOLDER: &str = "postponed_events";

    let path = env::current_dir()
        .context("unable get current dir")?
        .join(POSTPONED_EVENTS_FOLDER)
        .as_path()
        .into();
    Ok(path)
}

fn init_postponed_events_dir(
    postponed_events_dir_from_settings: Option<PathBuf>,
) -> Result<Arc<Path>> {
    let path = get_postponed_events_dir(postponed_events_dir_from_settings)?;

    create_dir_all(path.clone()).context("unable create `postponed_events` dir")?;

    Ok(path)
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct PostponedEventsFileFormat {
    version: u32,
    table_name: String,
    events: Vec<InsertEvent>,
}

impl PostponedEventsFileFormat {
    pub fn new(table_name: String, events: Vec<InsertEvent>) -> Self {
        Self {
            version: 1,
            table_name,
            events,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EventRecorderFallback {
    postponed_events_dir: Arc<Path>,
}

impl EventRecorderFallback {
    /// EventRecorder's fallback handlers
    /// postponed_events_dir: postponed events director from settings if exists
    pub fn new(postponed_events_dir: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            postponed_events_dir: init_postponed_events_dir(postponed_events_dir)?,
        })
    }

    pub(crate) async fn save_to_file(
        &self,
        table_name: String,
        not_written_events: Vec<InsertEvent>,
    ) -> Result<()> {
        let postponed_events_dir = self.postponed_events_dir.clone();
        spawn_blocking(move || -> Result<()> {
            let now = timeout_manager::now();

            let file_names = FileNames::from_date(now);
            let not_finished_file_path = postponed_events_dir.join(&file_names.not_finished);

            let file = File::create(not_finished_file_path.clone())
                .context("can't create file for postponed events")?;
            let mut buf_writer = BufWriter::with_capacity(BUFFER_SIZE, file);

            let file_format = PostponedEventsFileFormat::new(table_name, not_written_events);
            serde_json::to_writer(&mut buf_writer, &file_format).with_context(|| {
                format!(
                    "failed saving postponed events to file `{}`",
                    not_finished_file_path.display()
                )
            })?;

            let finished_file_path = postponed_events_dir.join(&file_names.finished);
            fs::rename(not_finished_file_path, finished_file_path).with_context(|| {
                format!(
                    "can't rename from {} to {}",
                    file_names.not_finished, file_names.finished,
                )
            })?;

            Ok(())
        })
        .await??;

        Ok(())
    }

    pub(crate) async fn get_existing_postponed_events_file_names(&self) -> Result<Vec<OsString>> {
        let postponed_events_dir = self.postponed_events_dir.clone();
        spawn_blocking(move || -> Result<_> {
            let read_dir_res = fs::read_dir(postponed_events_dir);

            let vec = read_dir_res?
                .filter_map(select_events_file_names)
                .collect_vec();
            Ok(vec)
        })
        .await?
    }

    pub(crate) async fn try_restore_to_db_postponed_events(
        &self,
        pool: &PgPool,
        file_names: &[OsString],
    ) {
        for file_name in file_names {
            let file_path = self.postponed_events_dir.join(file_name);

            let PostponedEventsFileFormat {
                table_name, events, ..
            } = match load_from_file(file_path.clone()).await {
                Ok(file) => file,
                Err(err) => {
                    let path = file_path.display();
                    log::error!("failed load postponed events file {path} with error: {err:?}");
                    continue;
                }
            };

            match save_batch(pool, &table_name, events, self).await {
                Err(err) => log::error!("failed resaving batch of events to db: {err}"),
                Ok(()) => tokio::fs::remove_file(file_path)
                    .await
                    .unwrap_or_else(|err| {
                        log::error!("failed removing file from postponed events: {err}")
                    }),
            }
        }
    }
}

struct FileNames {
    not_finished: String,
    finished: String,
}

impl FileNames {
    fn from_date(now: DateTime) -> FileNames {
        let formatted_datetime = now.format("%Y.%m.%d_%H.%M.%S.%6f");
        FileNames {
            not_finished: format!(
                "{NOT_FINISHED_FILED_PREFIX}{EVENTS_FILE_PREFIX}{formatted_datetime}"
            ),
            finished: format!("{EVENTS_FILE_PREFIX}{formatted_datetime}"),
        }
    }
}

fn select_events_file_names(entry: std::io::Result<DirEntry>) -> Option<OsString> {
    let dir_entry = entry
        .map_err(|err| log::warn!("Can't read metadata of events file with error: {err}"))
        .ok()?;

    let file_type = dir_entry
        .file_type()
        .map_err(|err| {
            log::warn!(
                "Can't read file type of {} with error: {err}",
                dir_entry.file_name().to_string_lossy()
            )
        })
        .ok()?;

    if file_type.is_file() {
        let file_name = dir_entry.file_name();
        if file_name
            .as_os_str()
            .to_string_lossy()
            .starts_with(EVENTS_FILE_PREFIX)
        {
            return Some(file_name);
        }
        return None;
    }
    None
}

async fn load_from_file(path: PathBuf) -> Result<PostponedEventsFileFormat> {
    spawn_blocking(move || -> Result<_> {
        let file = File::open(&path)
            .with_context(|| format!("can't open postponed events file {}", path.display()))?;
        let reader = BufReader::with_capacity(BUFFER_SIZE, file);
        serde_json::from_reader(reader)
            .with_context(|| format!("can't read postponed events file {}", path.display()))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use crate::database::events::recorder::fallback::{
        load_from_file, EventRecorderFallback, PostponedEventsFileFormat,
    };
    use bb8_postgres::bb8::PooledConnection;
    use bb8_postgres::PostgresConnectionManager;
    use chrono::Utc;
    use mmb_database::impl_event;
    use mmb_database::postgres_db::events::{Event, InsertEvent};
    use mmb_database::postgres_db::tests::{get_database_url, PgPoolMutex};
    use mmb_utils::DateTime;
    use scopeguard::defer;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use tokio_postgres::NoTls;

    const TABLE_NAME: &str = "fallback_events";

    #[derive(Debug, Serialize, Deserialize)]
    pub struct TestEvent {
        date: DateTime,
    }

    impl_event!(TestEvent, TABLE_NAME);

    pub fn test_event() -> TestEvent {
        TestEvent { date: Utc::now() }
    }

    async fn init_test() -> PgPoolMutex {
        let pool_mutex = PgPoolMutex::create(&get_database_url(), 1).await;
        let connection = pool_mutex.pool.get_connection_expected().await;
        recreate_table(&connection).await;
        drop(connection);
        pool_mutex
    }

    async fn recreate_table<'a>(
        connection: &'a PooledConnection<'a, PostgresConnectionManager<NoTls>>,
    ) {
        let sql = include_str!(
            "../../../../../mmb_database/src/postgres_db/sql/create_or_truncate_table.sql"
        )
        .replace("TABLE_NAME", TABLE_NAME);

        connection
            .batch_execute(&sql)
            .await
            .expect("recreate table persons");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn save_files_on_fallback() {
        // arrange
        let fallback = EventRecorderFallback::new(None).expect("in test");
        defer! {
            fs::remove_dir_all(fallback.clone().postponed_events_dir).expect("clear postponed events dir");
        };

        let test_event = test_event();
        let test_event_data = InsertEvent {
            version: 0,
            json: test_event.get_json().expect("in test"),
        };

        let file_names = fallback
            .get_existing_postponed_events_file_names()
            .await
            .expect("in test");

        assert_eq!(
            file_names.len(),
            0,
            "must not have saved files at the test beginning"
        );

        // fallback saving to file
        fallback
            .save_to_file(TABLE_NAME.to_string(), vec![test_event_data.clone()])
            .await
            .expect("in test");

        // check saved
        let file_names = fallback
            .get_existing_postponed_events_file_names()
            .await
            .expect("in test");
        assert_eq!(file_names.len(), 1, "should exists 1 postponed events file");

        let file_path = fallback
            .postponed_events_dir
            .join(file_names.first().expect("should exists"));
        let file_format = load_from_file(file_path).await.expect("in test");

        let expected = PostponedEventsFileFormat {
            version: 1,
            table_name: TABLE_NAME.to_string(),
            events: vec![test_event_data],
        };
        pretty_assertions::assert_eq!(file_format, expected);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restore_postponed_events_in_db() {
        let pool_mutex = init_test().await;

        let fallback = EventRecorderFallback::new(None).expect("in test");
        defer! {
            fs::remove_dir_all(fallback.clone().postponed_events_dir).expect("clear postponed events dir");
        };

        let test_event = test_event();
        let test_event_data = InsertEvent {
            version: 0,
            json: test_event.get_json().expect("in test"),
        };

        let file_names = fallback
            .get_existing_postponed_events_file_names()
            .await
            .expect("in test");

        assert_eq!(
            file_names.len(),
            0,
            "must not have saved files at the test beginning"
        );

        fallback
            .save_to_file(TABLE_NAME.to_string(), vec![test_event_data.clone()])
            .await
            .expect("in test");

        // fallback saving to postponed events to DB
        let file_names = fallback
            .get_existing_postponed_events_file_names()
            .await
            .expect("in test");

        fallback
            .try_restore_to_db_postponed_events(&pool_mutex.pool, &file_names)
            .await;

        // check saved event to db
        let connection = pool_mutex.pool.get_connection_expected().await;
        let rows = connection
            .query(&format!("SELECT * FROM {TABLE_NAME}"), &[])
            .await
            .expect("select persons");

        assert_eq!(rows.len(), 1);
        let json: serde_json::Value = rows[0].get("json");
        pretty_assertions::assert_eq!(json, test_event_data.json);
    }
}
