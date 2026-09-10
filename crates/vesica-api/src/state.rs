//! Shared state handed to every handler via axum's `State` extractor.

use std::path::PathBuf;
use std::sync::Arc;

use vesica_store::Db;

use crate::config::AuthConfig;

/// State shared across every request. Cheap to clone: `Db` is itself a
/// clone-cheap handle (see `vesica_store::Db`), and everything else here is
/// wrapped in `Arc`.
#[derive(Clone)]
pub struct AppState {
    pub(crate) db: Db,
    pub(crate) blob_dir: Arc<PathBuf>,
    pub(crate) auth: Arc<AuthConfig>,
    /// The highest migration version this build of Vesica knows about
    /// (`vesica_store::EMBEDDED_MIGRATIONS`'s maximum), computed once at
    /// startup. The readiness check compares the database's actual applied
    /// version against this rather than hard-coding a number, so it can
    /// never silently fall out of sync with the migration set it's meant to
    /// be checking.
    pub(crate) expected_migration_version: i64,
}

impl AppState {
    pub(crate) fn new(db: Db, blob_dir: PathBuf, auth: AuthConfig) -> Self {
        let expected_migration_version = vesica_store::EMBEDDED_MIGRATIONS
            .iter()
            .map(|m| m.version)
            .max()
            .unwrap_or(0);
        AppState {
            db,
            blob_dir: Arc::new(blob_dir),
            auth: Arc::new(auth),
            expected_migration_version,
        }
    }
}
