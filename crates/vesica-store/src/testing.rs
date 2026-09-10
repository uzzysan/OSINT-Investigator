//! The shared test harness: one call that stands up a store against a fresh
//! database in a temporary directory, and tears it down when the caller is
//! done.
//!
//! This is what the ticket calls "the test harness the rest of the
//! milestone depends on... Later tickets should not each invent their own."
//! Call [`test_store`] rather than hand-rolling a `TempDir` + `Db::open`
//! pair:
//!
//! ```no_run
//! # async fn example() -> Result<(), vesica_store::StoreError> {
//! let store = vesica_store::testing::test_store().await?;
//! let cases = vesica_store::repo::case::list(store.db.read_pool(), &Default::default()).await?;
//! // ... exercise the store against a real, freshly-migrated SQLite database ...
//! # Ok(())
//! # }
//! // `store` (and its temp directory) is dropped and cleaned up here.
//! ```
//!
//! `test_store` is a normal async function, not a `#[cfg(test)]` item, so it
//! is reachable from other crates' integration tests and `#[tokio::test]`
//! functions (e.g. ticket 08) the same way any other public API is:
//! `vesica_store::testing::test_store()`. Its only unusual dependency,
//! `tempfile`, is therefore a normal (not dev-) dependency of this crate —
//! see the note next to it in `Cargo.toml`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::error::StoreError;
use crate::ids;
use crate::repo::case::{self, CaseRow};
use crate::store::Db;

/// A store backed by a database in its own temporary directory. Dropping
/// this removes the directory (and the SQLite file, WAL and SHM files
/// inside it), so callers need no explicit teardown.
pub struct TestStore {
    /// The live store: writer task running, reader pool open, schema
    /// migrated to the latest version.
    pub db: Db,
    dir: TempDir,
}

impl TestStore {
    /// The path of the SQLite file backing this store, inside its temporary
    /// directory. Mostly useful for tests that want to open a second,
    /// independent connection to the same file (e.g. to prove WAL allows
    /// concurrent readers).
    pub fn db_path(&self) -> PathBuf {
        self.dir.path().join("vesica.sqlite")
    }

    /// The temporary directory itself.
    pub fn dir_path(&self) -> &Path {
        self.dir.path()
    }
}

/// Stands up a store against a fresh SQLite database in a new temporary
/// directory, with every migration applied, the writer task running and the
/// reader pool open. The directory is removed automatically when the
/// returned [`TestStore`] is dropped.
pub async fn test_store() -> Result<TestStore, StoreError> {
    let dir = TempDir::new()?;
    let db_path = dir.path().join("vesica.sqlite");
    let (db, _outcome) = Db::open(&db_path).await?;
    Ok(TestStore { db, dir })
}

/// Inserts a minimal live case and returns the row, for tests in this crate
/// that need something for an entity, relation or source to hang off. It
/// writes no audit entry, because it is not a use case — the service layer
/// owns that, and a store-level test asserting foreign keys work does not
/// want a service to exist first.
pub async fn seed_case(db: &Db, slug: &str) -> Result<CaseRow, StoreError> {
    let now = ids::now();
    let row = CaseRow {
        id: ids::new_id(),
        slug: slug.to_string(),
        name: slug.to_string(),
        description: None,
        color: None,
        status: "active".to_string(),
        default_region: None,
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    };
    let stored = row.clone();
    db.transaction(move |conn| Box::pin(async move { case::insert(conn, &stored).await }))
        .await?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_is_isolated_and_cleans_up_its_directory() {
        let dir_path;
        {
            let store = test_store().await.unwrap();
            dir_path = store.dir_path().to_path_buf();
            assert!(dir_path.exists());
            assert!(store.db_path().exists(), "the sqlite file should exist");

            // A second, independent call gets its own directory and its own
            // database — no shared state between them.
            let other = test_store().await.unwrap();
            assert_ne!(store.dir_path(), other.dir_path());
        }
        assert!(
            !dir_path.exists(),
            "the temp directory must be gone once the TestStore is dropped"
        );
    }
}
