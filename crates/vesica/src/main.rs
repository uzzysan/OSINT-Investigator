//! `vesica`: the Vesica binary.
//!
//! Owns configuration (environment plus `.env`), structured logging, and the
//! CLI subcommands (`serve`, `mcp`, `migrate`, `rescan-matches`, `backup`,
//! `check`, `import`, `export`). See `03-architecture.md` section 3.4.
//!
//! Ticket 0001-02 gave this binary its real configuration loading,
//! validation and logging setup — that work happens for every invocation
//! below. `migrate` (0001-03) and `serve` (0001-04) now have real
//! behaviour too. Every remaining subcommand still depends on a service
//! layer (`vesica-mcp`, `vesica-import`, `vesica-report`, ...) that is only
//! a crate boundary today, with behaviour landing in later tickets, so each
//! of those prints a clear "not implemented yet" message naming the ticket
//! that owns it and exits non-zero, rather than pretending to do work it
//! can't yet do.

#![forbid(unsafe_code)]

mod cli;
mod config;
mod logging;

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command};
use config::{AuthMode, Config};
use vesica_api::{ApiConfig, AuthConfig, ServeError};

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
        Command::Serve => run_serve(config),
        Command::Migrate => run_migrate(config),
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

/// `vesica migrate`: opens the database at `config.db_path`, applying every
/// pending migration (see `vesica-store`'s migration runner), and exits with
/// a status reflecting whether that succeeded. A second run against an
/// already-current database succeeds and reports a no-op — see
/// `vesica-store::migrate` for the transactional/no-op guarantees.
///
/// This subcommand does its own small single-threaded async runtime rather
/// than making `main` itself async: nothing else in the binary needs async
/// yet (`serve`, the only other command that will, is ticket 0001-04), and
/// confining the runtime to this one command keeps the rest of `main`
/// exactly as ticket 0001-02 left it.
fn run_migrate(config: &Config) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "failed to start the async runtime migrate needs");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async {
        match vesica_store::Db::open(&config.db_path).await {
            Ok((_db, outcome)) => {
                match outcome {
                    vesica_store::MigrationOutcome::NoOp { current_version } => {
                        tracing::info!(
                            current_version,
                            db_path = %config.db_path.display(),
                            "database already up to date; nothing to migrate"
                        );
                    }
                    vesica_store::MigrationOutcome::Applied {
                        applied,
                        current_version,
                    } => {
                        tracing::info!(
                            ?applied,
                            current_version,
                            db_path = %config.db_path.display(),
                            "migrations applied"
                        );
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    db_path = %config.db_path.display(),
                    "migration failed; schema left as it was before this run"
                );
                ExitCode::FAILURE
            }
        }
    })
}

/// `vesica serve` (also the default with no subcommand): opens the database
/// at `config.db_path` — applying any pending migration exactly as
/// `vesica migrate` does, since `vesica_store::Db::open` is the one place
/// that happens — then runs the HTTP server from `vesica-api` until it
/// receives `SIGINT`/`SIGTERM`. See `vesica_api::serve` for what it binds
/// and serves, and `03-architecture.md` section 3.4 ("vesica").
///
/// Uses a multi-thread runtime, unlike `run_migrate`'s single-threaded one:
/// this command has to accept connections, run the auth middleware and
/// handlers, and wait on OS signals all at once, for as long as the process
/// runs, so it benefits from more than one worker thread the way a one-shot
/// command that opens a database and exits does not.
fn run_serve(config: &Config) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "failed to start the async runtime serve needs");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async {
        let db = match vesica_store::Db::open(&config.db_path).await {
            Ok((db, outcome)) => {
                match outcome {
                    vesica_store::MigrationOutcome::NoOp { current_version } => {
                        tracing::info!(
                            current_version,
                            db_path = %config.db_path.display(),
                            "database already up to date"
                        );
                    }
                    vesica_store::MigrationOutcome::Applied {
                        applied,
                        current_version,
                    } => {
                        tracing::info!(
                            ?applied,
                            current_version,
                            db_path = %config.db_path.display(),
                            "migrations applied"
                        );
                    }
                }
                db
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    db_path = %config.db_path.display(),
                    "could not open the database; refusing to start"
                );
                return ExitCode::FAILURE;
            }
        };

        let auth = match config.auth_mode {
            AuthMode::Solo => AuthConfig::Solo,
            AuthMode::Token => AuthConfig::Token(config.api_token.expose_secret().to_string()),
        };
        let api_config = ApiConfig {
            bind_addr: config.bind_addr,
            port: config.port,
            blob_dir: config.blob_dir.clone(),
            auth,
        };

        match vesica_api::serve(api_config, db).await {
            Ok(()) => {
                tracing::info!("vesica serve: shut down cleanly");
                ExitCode::SUCCESS
            }
            // The configured port is already bound by something else. Per
            // 08-deployment.md, hunting for a free port is an
            // installer-time decision only — a running service must never
            // make that call itself, since a silently different port
            // breaks every agent and browser bookmark pointed at the
            // configured one. So this is a clear, non-zero exit, not a
            // fallback bind.
            Err(e @ ServeError::AddrInUse(_)) => {
                tracing::error!(error = %e, "vesica serve: cannot start");
                ExitCode::FAILURE
            }
            Err(e) => {
                tracing::error!(error = %e, "vesica serve: cannot start");
                ExitCode::FAILURE
            }
        }
    })
}
