//! Schema migrations: numbered SQL files applied, in order, inside a single
//! transaction.
//!
//! An operator who kills the process mid-migration and restarts finds
//! either the old schema or the new one, never a half-applied hybrid: every
//! migration still pending at the start of a [`run`] call is executed and
//! committed together, or the first failure rolls the whole batch back and
//! nothing changes. Re-running against an already-current database is a
//! genuine no-op — it never opens a write transaction at all.
//!
//! The six files under `/migrations` at the workspace root
//! ([`EMBEDDED_MIGRATIONS`]) are embedded into the binary at compile time via
//! `include_str!`, so the built binary carries its own schema and needs no
//! external migrations directory at runtime (03-architecture.md 3.2,
//! "embedded web assets" — the same reasoning applies here). This is a
//! plain `include_str!`, not one of the `sqlx::query!`-family compile-time
//! macros AGENT-BRIEF.md rules out: it does no SQL validation and needs no
//! `DATABASE_URL`, it just reads a file's bytes at build time.

use std::collections::BTreeSet;

use sqlx::{Connection, Executor, SqliteConnection};

use crate::error::StoreError;
use crate::ids;

/// One numbered migration: a version, a name (for logging and error
/// messages) and the SQL to run.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// The migration's ordinal, matching its file's numeric prefix (e.g. `1`
    /// for `0001_core.sql`). Applied in ascending order; recorded in
    /// `schema_migration` once committed.
    pub version: i64,
    /// The file's name, without the number or extension (e.g. `"core"`).
    pub name: &'static str,
    /// The migration's full SQL, one or more statements separated by `;`.
    pub sql: &'static str,
}

/// The full set of migrations Vesica ships, in the order they must apply.
/// See the module docs for why these are `include_str!`'d rather than read
/// from disk at runtime.
pub const EMBEDDED_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "core",
        sql: include_str!("../../../migrations/0001_core.sql"),
    },
    Migration {
        version: 2,
        name: "evidence",
        sql: include_str!("../../../migrations/0002_evidence.sql"),
    },
    Migration {
        version: 3,
        name: "timeline",
        sql: include_str!("../../../migrations/0003_timeline.sql"),
    },
    Migration {
        version: 4,
        name: "matching",
        sql: include_str!("../../../migrations/0004_matching.sql"),
    },
    Migration {
        version: 5,
        name: "search",
        sql: include_str!("../../../migrations/0005_search.sql"),
    },
    Migration {
        version: 6,
        name: "bookkeeping",
        sql: include_str!("../../../migrations/0006_bookkeeping.sql"),
    },
];

/// What a [`run`] call did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// The database was already at the latest version among the migrations
    /// passed in; nothing was applied and no write transaction was opened.
    NoOp {
        /// The highest version already recorded as applied (`0` for a
        /// brand-new database with no migrations at all, which is only
        /// possible if `migrations` was empty).
        current_version: i64,
    },
    /// These migrations, in this order, were newly applied and committed
    /// together.
    Applied {
        /// Versions applied by this call, in the order they ran.
        applied: Vec<i64>,
        /// The highest version now recorded as applied.
        current_version: i64,
    },
}

/// SQL creating the bookkeeping table that records which migrations have
/// been applied. `CREATE TABLE IF NOT EXISTS` is itself a single atomic
/// DDL statement, so this is safe to run unconditionally on every call.
const CREATE_BOOKKEEPING_TABLE: &str = "
CREATE TABLE IF NOT EXISTS schema_migration (
  version    INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  applied_at TEXT NOT NULL
)";

/// Applies every migration in `migrations` not yet recorded in the
/// `schema_migration` bookkeeping table, in ascending version order.
///
/// All pending migrations run inside one transaction: either every one of
/// them applies and the transaction commits, or the first failure rolls the
/// whole batch back and the schema is exactly as it was before this call —
/// see the module docs. Takes the migration list as a parameter (rather than
/// always using [`EMBEDDED_MIGRATIONS`]) so tests can exercise the failure
/// path with a deliberately broken migration without touching the real
/// schema.
pub async fn run(
    conn: &mut SqliteConnection,
    migrations: &[Migration],
) -> Result<MigrationOutcome, StoreError> {
    conn.execute(CREATE_BOOKKEEPING_TABLE).await?;

    let applied_versions: BTreeSet<i64> =
        sqlx::query_scalar("SELECT version FROM schema_migration ORDER BY version")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();

    let mut ordered: Vec<&Migration> = migrations.iter().collect();
    ordered.sort_by_key(|m| m.version);

    let pending: Vec<&Migration> = ordered
        .into_iter()
        .filter(|m| !applied_versions.contains(&m.version))
        .collect();

    let current_version = applied_versions.iter().max().copied().unwrap_or(0);

    if pending.is_empty() {
        return Ok(MigrationOutcome::NoOp { current_version });
    }

    let mut tx = conn.begin().await?;
    let mut applied = Vec::with_capacity(pending.len());
    for migration in &pending {
        sqlx::raw_sql(migration.sql)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                StoreError::Migration(format!(
                    "{:04}_{} failed: {e}",
                    migration.version, migration.name
                ))
            })?;

        sqlx::query("INSERT INTO schema_migration (version, name, applied_at) VALUES (?, ?, ?)")
            .bind(migration.version)
            .bind(migration.name)
            .bind(ids::now())
            .execute(&mut *tx)
            .await?;

        applied.push(migration.version);
    }
    tx.commit().await?;

    let current_version = *applied.last().expect("pending is non-empty");
    Ok(MigrationOutcome::Applied {
        applied,
        current_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;
    use sqlx::sqlite::SqliteConnectOptions;

    async fn open_memory() -> SqliteConnection {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(":memory:")
                .foreign_keys(true),
        )
        .await
        .expect("open in-memory sqlite")
    }

    #[tokio::test]
    async fn applying_all_embedded_migrations_creates_every_table_and_the_fts_index() {
        let mut conn = open_memory().await;
        let outcome = run(&mut conn, EMBEDDED_MIGRATIONS).await.unwrap();
        assert!(matches!(
            outcome,
            MigrationOutcome::Applied {
                current_version: 6,
                ..
            }
        ));

        let expected_tables = [
            "kase",
            "entity",
            "entity_prop",
            "relation",
            "relation_prop",
            "source",
            "blob",
            "attachment",
            "citation",
            "observation",
            "match_job",
            "block_key",
            "match_candidate",
            "entity_link",
            "tag",
            "entity_tag",
            "audit_log",
            "schema_def",
            "saved_view",
            "schema_migration",
        ];
        for table in expected_tables {
            let row =
                sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
                    .bind(table)
                    .fetch_optional(&mut conn)
                    .await
                    .unwrap();
            assert!(row.is_some(), "table {table} should exist after migrating");
        }

        let fts = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'entity_fts'",
        )
        .fetch_optional(&mut conn)
        .await
        .unwrap();
        assert!(fts.is_some(), "entity_fts virtual table should exist");

        let triggers: Vec<String> =
            sqlx::query("SELECT name FROM sqlite_master WHERE type = 'trigger'")
                .fetch_all(&mut conn)
                .await
                .unwrap()
                .into_iter()
                .map(|r| r.get::<String, _>("name"))
                .collect();
        assert_eq!(
            triggers.len(),
            6,
            "expected the six entity_fts sync triggers, got {triggers:?}"
        );
    }

    #[tokio::test]
    async fn rerunning_against_an_up_to_date_database_is_a_no_op() {
        let mut conn = open_memory().await;
        run(&mut conn, EMBEDDED_MIGRATIONS).await.unwrap();

        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migration")
            .fetch_one(&mut conn)
            .await
            .unwrap();

        let outcome = run(&mut conn, EMBEDDED_MIGRATIONS).await.unwrap();
        assert_eq!(outcome, MigrationOutcome::NoOp { current_version: 6 });

        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_migration")
            .fetch_one(&mut conn)
            .await
            .unwrap();
        assert_eq!(
            before, after,
            "a no-op run must not touch the bookkeeping table"
        );
    }

    #[tokio::test]
    async fn a_failing_migration_leaves_the_previous_schema_completely_intact() {
        let mut conn = open_memory().await;

        // Apply only the first migration for real, then attempt a batch
        // containing one good migration followed by a deliberately broken
        // one, in a single `run` call — proving the whole pending batch is
        // one transaction, not one transaction per file.
        let first = EMBEDDED_MIGRATIONS[0];
        run(&mut conn, &[first]).await.unwrap();

        let broken = Migration {
            version: 3,
            name: "broken",
            sql: "CREATE TABLE this_should_not_exist (id TEXT PRIMARY KEY); \
                  THIS IS NOT VALID SQL;",
        };
        let good_second = Migration {
            version: 2,
            name: "also_should_not_land",
            sql: "CREATE TABLE also_should_not_exist (id TEXT PRIMARY KEY);",
        };

        let result = run(&mut conn, &[first, good_second, broken]).await;
        assert!(
            result.is_err(),
            "a broken migration must be reported as an error"
        );

        // The table from the syntactically-valid migration that ran earlier
        // in the same failed batch must not have survived the rollback.
        let leaked = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'also_should_not_exist'",
        )
        .fetch_optional(&mut conn)
        .await
        .unwrap();
        assert!(
            leaked.is_none(),
            "the good migration in the failed batch must have rolled back too"
        );

        let broken_table = sqlx::query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'this_should_not_exist'",
        )
        .fetch_optional(&mut conn)
        .await
        .unwrap();
        assert!(broken_table.is_none());

        // Bookkeeping must still show only the one migration applied before
        // the failing run.
        let applied: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM schema_migration ORDER BY version")
                .fetch_all(&mut conn)
                .await
                .unwrap();
        assert_eq!(applied, vec![first.version]);

        // And the database must still be perfectly usable: applying the
        // rest of the real migrations (skipping the fictitious version 2/3
        // above) from here must succeed cleanly.
        let rest: Vec<Migration> = EMBEDDED_MIGRATIONS[1..].to_vec();
        let outcome = run(&mut conn, &rest).await.unwrap();
        assert!(matches!(
            outcome,
            MigrationOutcome::Applied {
                current_version: 6,
                ..
            }
        ));
    }
}
