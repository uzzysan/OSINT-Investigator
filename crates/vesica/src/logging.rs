//! Structured logging to stdout, in a form journald can read.
//!
//! The level and format (plain text or JSON) come from [`crate::config::Config`]
//! — no rebuild needed to change either. Uses `try_init` rather than `init`
//! so this is safe to call more than once within a process (tests do).

use tracing_subscriber::EnvFilter;

use crate::config::{Config, LogFormat};

/// Installs the global `tracing` subscriber described by `config`. Safe to
/// call more than once in a process; later calls are no-ops.
pub fn init(config: &Config) {
    let filter =
        EnvFilter::try_new(config.log_level.to_string()).unwrap_or_else(|_| EnvFilter::new("info"));

    let result = match config.log_format {
        LogFormat::Json => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .json()
            .try_init(),
        LogFormat::Text => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .try_init(),
    };

    if let Err(e) = result {
        // Only happens if a subscriber is already installed (e.g. a second
        // call in the same process, or a test harness). Not fatal.
        eprintln!("logging: subscriber already installed, keeping the existing one ({e})");
    }
}
