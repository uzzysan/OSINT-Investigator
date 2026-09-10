//! The single error type every `vesica-store` operation returns.

use thiserror::Error;

/// Everything that can go wrong opening, migrating, reading from or writing
/// to the store.
#[derive(Debug, Error)]
pub enum StoreError {
    /// A `sqlx`/SQLite error surfaced verbatim, including foreign-key and
    /// other constraint violations.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// A migration failed to apply. Carries the migration's version and
    /// name so an operator can find the offending file without guessing.
    #[error("migration failed: {0}")]
    Migration(String),

    /// The single-writer task is gone (its channel closed or it panicked).
    /// Every write goes through it, so this is unrecoverable for the
    /// lifetime of the process.
    #[error("the writer task is not available")]
    WriterUnavailable,

    /// A filesystem operation failed (e.g. creating the temp directory a
    /// test store lives in, or the database's parent directory).
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}
