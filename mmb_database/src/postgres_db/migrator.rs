use anyhow::Context;
use futures::future::{join_all, BoxFuture};
use futures::FutureExt;
use itertools::Itertools;
use sqlx::error::BoxDynError;
use sqlx::migrate::{Migration, MigrationSource, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::path::PathBuf;

#[derive(Debug)]
struct MigrationSources {
    migration_sources: Vec<PathBuf>,
}

impl<'s> MigrationSource<'s> for MigrationSources {
    fn resolve(self) -> BoxFuture<'s, Result<Vec<Migration>, BoxDynError>> {
        async move {
            let mut migrations: Vec<_> =
                join_all(self.migration_sources.iter().map(|p| async move {
                    let path = p.as_path();
                    let res = path.resolve().await;
                    res.map_err(|err| {
                        let path = path.to_string_lossy();
                        format!("failed resolving migrations by path: '{path} . {err}'")
                    })
                }))
                .await
                .into_iter()
                .flatten_ok()
                .try_collect()?;

            migrations.sort_by_key(|i| i.version);
            Ok(migrations)
        }
        .boxed()
    }
}

/// Run migrations from list of specified sources
pub async fn apply_migrations(
    database_url: &str,
    migration_sources: Vec<PathBuf>,
) -> anyhow::Result<()> {
    let migrator = Migrator::new(MigrationSources { migration_sources }).await?;
    let connection_pool = create_connection_pool(database_url, 2).await?;
    migrator.run(&connection_pool).await?;
    Ok(())
}

async fn create_connection_pool(
    database_url: &str,
    max_connections: u32,
) -> anyhow::Result<Pool<Postgres>> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .context("Unable to connect to DB")
}

#[cfg(test)]
mod tests {
    use super::apply_migrations;
    use crate::postgres_db::migrator::create_connection_pool;
    use crate::postgres_db::tests::get_database_url;
    use itertools::Itertools;
    use ntest::timeout;
    use sqlx::{Pool, Postgres};
    use std::env;
    use std::path::PathBuf;

    const DATABASE_URL: &str = "postgres://postgres:postgres@localhost/tests";
    const TABLE_NAME1: &str = "test_mig1";
    const TABLE_NAME2: &str = "test_mig2";

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[timeout(20_000)]
    async fn test_apply_undo_migrations() {
        init_test().await;

        let sql_dir = get_project_root_dir().join("mmb_database/src/postgres_db/sql");

        let sources = [
            "first_test_migrations/migrations",
            "second_migrations/migrations",
        ]
        .iter()
        .map(|p| sql_dir.clone().join(p))
        .collect_vec();

        apply_migrations(&get_database_url(), sources)
            .await
            .expect("failed apply_migrations in test");

        let pool = create_connection_pool(&get_database_url(), 2)
            .await
            .expect("failed create_connection_pool in test");

        let rows1 = sqlx::query::<Postgres>(&format!("SELECT * FROM {TABLE_NAME1}"))
            .fetch_all(&pool)
            .await
            .expect("failed select from table1 in test");

        assert_eq!(rows1.len(), 0);

        let rows2 = sqlx::query::<Postgres>(&format!("SELECT * FROM {TABLE_NAME2}"))
            .fetch_all(&pool)
            .await
            .expect("failed select from table1 in test");

        assert_eq!(rows2.len(), 0);

        clean_db(&pool).await;
    }

    fn get_project_root_dir() -> PathBuf {
        env::current_exe()
            .expect("in test")
            .iter()
            .take_while(|p| p.to_string_lossy() != "target")
            .collect::<PathBuf>()
    }

    async fn init_test() {
        let pool = create_connection_pool(&get_database_url(), 2)
            .await
            .expect("failed create_connection_pool in test");

        clean_db(&pool).await;
    }

    async fn clean_db(pool: &Pool<Postgres>) {
        sqlx::query("do $$ begin if (select 1 from pg_class where relname = '_sqlx_migrations' and pg_table_is_visible(oid)) then truncate table _sqlx_migrations; end if; end $$;")
            .execute(pool)
            .await
            .expect("failed truncate table _sqlx_migrations");

        sqlx::query(&format!("DROP TABLE IF EXISTS {TABLE_NAME1}"))
            .execute(pool)
            .await
            .expect("failed drop table1");

        sqlx::query(&format!("DROP TABLE IF EXISTS {TABLE_NAME2}"))
            .execute(pool)
            .await
            .expect("failed drop table2");
    }
}
