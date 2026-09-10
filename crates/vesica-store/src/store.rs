//! Opens the Vesica SQLite database and wires the concurrency model from
//! 03-architecture.md section 3.7: migrations run first on a dedicated
//! connection, then a single writer task and a reader pool are started,
//! both using identical connection settings.

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Connection, SqliteConnection, SqlitePool};

use crate::error::StoreError;
use crate::migrate::{self, Migration, MigrationOutcome};
use crate::writer::{self, WriterHandle};

/// How long a connection waits on `SQLITE_BUSY` before giving up. This is a
/// safety net, not the mechanism: the single-writer/reader-pool design makes
/// write/write contention structurally impossible, and readers in WAL mode
/// never block on a writer. If this timeout is ever hit in practice it means
/// the design assumption broke, not that a retry loop is needed — which is
/// why there is no retry logic anywhere in this crate.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// How many concurrent reader connections the pool maintains. Reads never
/// contend with the single writer (WAL mode), so this only bounds how many
/// reads can be in flight at once, not correctness.
const READER_POOL_SIZE: u32 = 8;

/// The store: a handle to the single writer task plus a pool of reader
/// connections. Cheap to clone (an `Arc`-backed channel sender and a
/// `sqlx::Pool`, both already clone-cheap); repositories hold a `Db` each.
#[derive(Clone)]
pub struct Db {
    writer: WriterHandle,
    read_pool: SqlitePool,
}

impl Db {
    /// Opens (creating the file if absent) the database at `path`, applies
    /// every pending migration from [`crate::migrate::EMBEDDED_MIGRATIONS`],
    /// then starts the writer task and reader pool. Safe to call against an
    /// already-current database: migrating is a no-op in that case (see
    /// [`crate::migrate::run`]).
    pub async fn open(path: &Path) -> Result<(Db, MigrationOutcome), StoreError> {
        Db::open_with(path, migrate::EMBEDDED_MIGRATIONS).await
    }

    /// As [`Db::open`], but against an explicit migration set. This is the
    /// seam the test suite uses to prove migration failure/rollback
    /// behaviour without touching the real embedded migrations, and it is
    /// also what [`crate::testing::test_store`] calls.
    pub async fn open_with(
        path: &Path,
        migrations: &[Migration],
    ) -> Result<(Db, MigrationOutcome), StoreError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let options = connect_options(path);

        // Migrations run on their own short-lived connection, before the
        // writer task or reader pool exist, so nothing can read a
        // half-migrated schema.
        let mut migration_conn = SqliteConnection::connect_with(&options).await?;
        let outcome = migrate::run(&mut migration_conn, migrations).await?;
        migration_conn.close().await?;

        let write_conn = SqliteConnection::connect_with(&options).await?;
        let writer = writer::spawn(write_conn);

        let read_pool = SqlitePoolOptions::new()
            .max_connections(READER_POOL_SIZE)
            .connect_with(options)
            .await?;

        Ok((Db { writer, read_pool }, outcome))
    }

    /// Runs `f` against the single write connection, serialised against
    /// every other write. See [`crate::writer::WriterHandle::write`].
    pub async fn write<F, T>(&self, f: F) -> Result<T, StoreError>
    where
        F: for<'c> FnOnce(&'c mut SqliteConnection) -> crate::BoxFuture<'c, Result<T, StoreError>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        self.writer.write(f).await
    }

    /// The reader pool. Repositories issue plain `SELECT`s against this;
    /// nothing routed through here should write, since the pool's
    /// connections are not the connection the writer task owns.
    pub fn read_pool(&self) -> &SqlitePool {
        &self.read_pool
    }
}

/// The pragmas required on every connection this crate opens (writer,
/// reader pool, and the one-off migration connection): WAL journalling,
/// foreign-key enforcement, `synchronous = NORMAL`, and a busy timeout.
/// Expressed as `SqliteConnectOptions` builder calls rather than raw
/// `PRAGMA` statements so a `sqlx::Pool` applies them to every connection it
/// opens automatically, including ones opened lazily under load.
fn connect_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn opening_twice_against_the_same_path_is_a_no_op_the_second_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("vesica.sqlite");

        let (_db1, outcome1) = Db::open(&path).await.unwrap();
        assert!(matches!(
            outcome1,
            MigrationOutcome::Applied {
                current_version: 6,
                ..
            }
        ));

        let (_db2, outcome2) = Db::open(&path).await.unwrap();
        assert_eq!(outcome2, MigrationOutcome::NoOp { current_version: 6 });
    }

    #[tokio::test]
    async fn every_connection_has_the_required_pragmas_set() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("vesica.sqlite");
        let (db, _) = Db::open(&path).await.unwrap();

        // Reader pool connection.
        let journal: String = sqlx::query("PRAGMA journal_mode")
            .fetch_one(db.read_pool())
            .await
            .unwrap()
            .get(0);
        assert_eq!(journal.to_lowercase(), "wal");

        let fk: i64 = sqlx::query("PRAGMA foreign_keys")
            .fetch_one(db.read_pool())
            .await
            .unwrap()
            .get(0);
        assert_eq!(fk, 1);

        let sync: i64 = sqlx::query("PRAGMA synchronous")
            .fetch_one(db.read_pool())
            .await
            .unwrap()
            .get(0);
        assert_eq!(sync, 1, "NORMAL is synchronous level 1");

        let busy_timeout: i64 = sqlx::query("PRAGMA busy_timeout")
            .fetch_one(db.read_pool())
            .await
            .unwrap()
            .get(0);
        assert!(busy_timeout > 0);

        // Writer connection: pragmas are per-connection, so prove the write
        // connection has them too, not just the pool.
        let (journal_w, fk_w): (String, i64) = db
            .write(|conn| {
                Box::pin(async move {
                    let j: String = sqlx::query("PRAGMA journal_mode")
                        .fetch_one(&mut *conn)
                        .await?
                        .get(0);
                    let f: i64 = sqlx::query("PRAGMA foreign_keys")
                        .fetch_one(&mut *conn)
                        .await?
                        .get(0);
                    Ok((j, f))
                })
            })
            .await
            .unwrap();
        assert_eq!(journal_w.to_lowercase(), "wal");
        assert_eq!(fk_w, 1);
    }

    #[tokio::test]
    async fn foreign_key_violations_are_rejected() {
        // This is the regression-prone one: if `foreign_keys=ON` is ever
        // dropped from `connect_options`, SQLite silently accepts orphaned
        // rows instead of erroring, and this test is what catches it.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("vesica.sqlite");
        let (db, _) = Db::open(&path).await.unwrap();

        let result = db
            .write(|conn| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO entity (id, case_id, schema_id, caption, created_at, created_by, updated_at) \
                         VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'no-such-case', 'Person', 'x', 'now', 'test', 'now')",
                    )
                    .execute(&mut *conn)
                    .await
                    .map_err(StoreError::from)
                })
            })
            .await;

        assert!(
            result.is_err(),
            "inserting an entity with a non-existent case_id must be rejected by foreign_keys=ON"
        );
    }

    #[tokio::test]
    async fn many_concurrent_writes_all_land_and_none_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("vesica.sqlite");
        let (db, _) = Db::open(&path).await.unwrap();

        const N: usize = 100;
        let mut handles = Vec::with_capacity(N);
        for i in 0..N {
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                let slug = format!("case-{i}");
                db.write(move |conn| {
                    Box::pin(async move {
                        sqlx::query(
                            "INSERT INTO kase (id, slug, name, status, created_at, updated_at) \
                             VALUES (?, ?, ?, 'active', 'now', 'now')",
                        )
                        .bind(format!("id-{i:04}"))
                        .bind(slug)
                        .bind(format!("Case {i}"))
                        .execute(&mut *conn)
                        .await
                        .map_err(StoreError::from)
                    })
                })
                .await
            }));
        }

        let mut errors = Vec::new();
        for handle in handles {
            if let Err(e) = handle.await.expect("task panicked") {
                errors.push(e.to_string());
            }
        }
        assert!(
            errors.is_empty(),
            "no concurrent write should error: {errors:?}"
        );

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kase")
            .fetch_one(db.read_pool())
            .await
            .unwrap();
        assert_eq!(count, N as i64, "every concurrent write must have landed");
    }
}
