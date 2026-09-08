//! `vesica`: the Vesica binary.
//!
//! Owns configuration (environment plus `.env`), structured logging, and the
//! CLI subcommands (`serve`, `mcp`, `migrate`, `rescan-matches`, `backup`,
//! `check`, `import`, `export`). See `03-architecture.md` section 3.4.
//!
//! Ticket 0001-02 gives this binary its real configuration loading,
//! validation and logging setup — that work happens for every invocation
//! below. The subcommand bodies themselves are still stubs: each depends on
//! a service layer (`vesica-store`, `vesica-api`, `vesica-mcp`, ...) that is
//! only a crate boundary today, with behaviour landing in later tickets. So
//! every subcommand prints a clear "not implemented yet" message naming the
//! ticket that owns it and exits non-zero, rather than pretending to do
//! work it can't yet do.

#![forbid(unsafe_code)]

mod cli;
mod config;
mod logging;

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command};
use config::{AuthMode, Config};

fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => {
            // Logging isn't set up yet (it depends on the very config that
            // just failed to load), so this goes straight to stderr.
            eprintln!("vesica: {e:#}");
            return ExitCode::FAILURE;
        }
    };

    logging::init(&config);
    log_startup_config(&config);

    let command = cli.command.unwrap_or(Command::Serve);
    run(command, &config)
}

/// Logs every configuration value at startup — proof, on every boot, that
/// loading and redaction both work: the token itself never appears, only
/// whether one is set and (when auth is on) how long it is.
fn log_startup_config(config: &Config) {
    tracing::info!(
        bind_addr = %config.bind_addr,
        port = config.port,
        public_url = %config.public_url,
        db_path = %config.db_path.display(),
        blob_dir = %config.blob_dir.display(),
        max_upload_mb = config.max_upload_mb,
        auth_mode = %config.auth_mode,
        mcp_http_enabled = config.mcp_http_enabled,
        er_suggest_threshold = config.er_suggest_threshold,
        er_strong_threshold = config.er_strong_threshold,
        er_cross_case = config.er_cross_case,
        log_level = %config.log_level,
        log_format = ?config.log_format,
        backup_enabled = config.backup_enabled,
        backup_dir = %config.backup_dir.display(),
        backup_keep = config.backup_keep,
        "configuration loaded"
    );

    if config.auth_mode == AuthMode::Token {
        tracing::info!(
            token_len = config.api_token.expose_secret().len(),
            "token auth enabled"
        );
    }
    tracing::debug!(?config, "full configuration (secrets redacted)");
}

/// Dispatches to each subcommand. Every subcommand is not-yet-implemented at
/// this stage of the project (see the module docs above); this function is
/// the single place that changes as later tickets give each one real
/// behaviour.
fn run(command: Command, config: &Config) -> ExitCode {
    let name = command.name();
    tracing::info!(subcommand = name, "dispatching");

    let not_yet = |owning_ticket: &str| {
        tracing::error!(
            subcommand = name,
            "vesica {name}: not implemented yet — {owning_ticket}. See 03-architecture.md section 3.4 for the intended behaviour."
        );
        ExitCode::FAILURE
    };

    match command {
        Command::Serve => {
            not_yet("planned for ticket 0001-04 (health, readiness and embedded web scaffold)")
        }
        Command::Migrate => not_yet(
            "planned for ticket 0001-03 (SQLite store, migrations and the single-writer runtime)",
        ),
        Command::Mcp => not_yet("MCP server wiring is not yet scheduled in a written ticket"),
        Command::RescanMatches => not_yet(
            "depends on vesica-resolve, which is not yet scheduled beyond its crate boundary",
        ),
        Command::Backup { out } => {
            // `--out` falls back to VESICA_BACKUP_DIR, same as the real
            // implementation will — worth resolving now so the not-yet
            // message tells the operator exactly where a backup would go.
            let target = out.unwrap_or_else(|| config.backup_dir.clone());
            tracing::info!(target = %target.display(), "resolved backup target directory");
            not_yet("depends on the SQLite store, which is not yet scheduled beyond ticket 0001-03")
        }
        Command::Check => not_yet(
            "depends on the SQLite store and blob store, which are not yet scheduled beyond their crate boundaries",
        ),
        Command::Import { .. } => not_yet(
            "depends on vesica-import, which is not yet scheduled beyond its crate boundary",
        ),
        Command::Export { .. } => not_yet(
            "depends on vesica-store and vesica-report, which are not yet scheduled beyond their crate boundaries",
        ),
    }
}
