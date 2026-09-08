//! The `vesica` command-line surface. See `03-architecture.md` section 3.4
//! ("vesica") for the subcommand table this mirrors. `serve` is the default
//! when no subcommand is given.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "vesica",
    version,
    about = "Vesica: self-hosted investigative knowledge graph with cross-case entity resolution and an MCP server."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the web + API + MCP HTTP server (default).
    Serve,

    /// Run the MCP server over stdio.
    Mcp,

    /// Apply migrations and exit.
    Migrate,

    /// Re-run entity resolution over everything.
    RescanMatches,

    /// Consistent backup of DB + blob manifest.
    Backup {
        /// Directory to write the backup into. Defaults to VESICA_BACKUP_DIR.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },

    /// Integrity check: blob hashes, orphans, FK validation.
    Check,

    /// Headless import.
    Import {
        /// Path to the file to import.
        file: PathBuf,
    },

    /// Headless export.
    Export {
        /// The case to export (id or slug).
        case: String,
    },
}

impl Command {
    /// The name as used on the command line, for messages that need to name
    /// the subcommand that produced them.
    pub fn name(&self) -> &'static str {
        match self {
            Command::Serve => "serve",
            Command::Mcp => "mcp",
            Command::Migrate => "migrate",
            Command::RescanMatches => "rescan-matches",
            Command::Backup { .. } => "backup",
            Command::Check => "check",
            Command::Import { .. } => "import",
            Command::Export { .. } => "export",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// clap's own structural sanity check: catches conflicting args,
    /// duplicate names, and other definition bugs before they'd surface at
    /// runtime.
    #[test]
    fn cli_definition_is_structurally_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_parses() {
        let cli = Cli::try_parse_from(["vesica"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn each_subcommand_parses() {
        assert!(matches!(
            Cli::try_parse_from(["vesica", "serve"]).unwrap().command,
            Some(Command::Serve)
        ));
        assert!(matches!(
            Cli::try_parse_from(["vesica", "mcp"]).unwrap().command,
            Some(Command::Mcp)
        ));
        assert!(matches!(
            Cli::try_parse_from(["vesica", "migrate"]).unwrap().command,
            Some(Command::Migrate)
        ));
        assert!(matches!(
            Cli::try_parse_from(["vesica", "rescan-matches"])
                .unwrap()
                .command,
            Some(Command::RescanMatches)
        ));
        assert!(matches!(
            Cli::try_parse_from(["vesica", "check"]).unwrap().command,
            Some(Command::Check)
        ));

        let backup = Cli::try_parse_from(["vesica", "backup", "--out", "/tmp/out"])
            .unwrap()
            .command;
        assert!(
            matches!(backup, Some(Command::Backup { out: Some(ref p) }) if p == std::path::Path::new("/tmp/out"))
        );

        let backup_default = Cli::try_parse_from(["vesica", "backup"]).unwrap().command;
        assert!(matches!(
            backup_default,
            Some(Command::Backup { out: None })
        ));

        let import = Cli::try_parse_from(["vesica", "import", "case.csv"])
            .unwrap()
            .command;
        assert!(
            matches!(import, Some(Command::Import { ref file }) if file == std::path::Path::new("case.csv"))
        );

        let export = Cli::try_parse_from(["vesica", "export", "case-123"])
            .unwrap()
            .command;
        assert!(matches!(export, Some(Command::Export { ref case }) if case == "case-123"));
    }

    #[test]
    fn import_without_a_file_argument_is_rejected() {
        assert!(Cli::try_parse_from(["vesica", "import"]).is_err());
    }

    #[test]
    fn export_without_a_case_argument_is_rejected() {
        assert!(Cli::try_parse_from(["vesica", "export"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        assert!(Cli::try_parse_from(["vesica", "not-a-real-command"]).is_err());
    }

    #[test]
    fn command_name_matches_the_cli_word() {
        assert_eq!(Command::Serve.name(), "serve");
        assert_eq!(Command::Mcp.name(), "mcp");
        assert_eq!(Command::Migrate.name(), "migrate");
        assert_eq!(Command::RescanMatches.name(), "rescan-matches");
        assert_eq!(Command::Backup { out: None }.name(), "backup");
        assert_eq!(Command::Check.name(), "check");
        assert_eq!(Command::Import { file: "x".into() }.name(), "import");
        assert_eq!(Command::Export { case: "x".into() }.name(), "export");
    }
}
