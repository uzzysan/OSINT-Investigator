//! Runtime configuration for the `vesica` binary.
//!
//! [`Config`] is loaded by `figment`, layering (lowest to highest precedence)
//! built-in defaults, then a `.env` file, then real process environment
//! variables — matching `env.example.txt`. Every key in that file has a
//! field here (`tests::all_env_example_keys_map_to_config_fields` checks the
//! two cannot silently diverge). A missing `.env` is not an error. Every
//! value is validated at load time: a bad value fails with a message naming
//! the offending `VESICA_*` key and what a valid value looks like, never a
//! panic and never a silent default. The API token is wrapped in
//! [`Redacted`] so it can never show up in a `{:?}` or log rendering by
//! accident.

use std::fmt;
use std::path::{Path, PathBuf};

use figment::value::{Dict, Map, Value};
use figment::{Figment, Metadata, Profile, Provider};

/// The `VESICA_` prefix every configuration key uses.
const ENV_PREFIX: &str = "VESICA_";

/// `(field key, default value)` pairs, one per line of `.env.example`. This
/// is the single source of truth both for defaults and for the test that
/// checks `.env.example` and this struct cannot silently diverge.
const DEFAULTS: &[(&str, &str)] = &[
    ("bind_addr", "127.0.0.1"),
    ("port", "8787"),
    ("public_url", "http://127.0.0.1:8787"),
    ("db_path", "./data/vesica.sqlite"),
    ("blob_dir", "./data/blobs"),
    ("max_upload_mb", "256"),
    ("auth_mode", "solo"),
    ("api_token", ""),
    ("mcp_http_enabled", "true"),
    ("er_suggest_threshold", "0.62"),
    ("er_strong_threshold", "0.85"),
    ("er_cross_case", "true"),
    ("log_level", "info"),
    ("log_format", "text"),
    ("backup_enabled", "true"),
    ("backup_dir", "./data/backups"),
    ("backup_keep", "14"),
];

/// Deployment access profile. `Solo` trusts the network it's bound to and
/// requires no login; `Token` requires a bearer token on every request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Solo,
    Token,
}

impl fmt::Display for AuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AuthMode::Solo => "solo",
            AuthMode::Token => "token",
        })
    }
}

/// `tracing` verbosity level, one-to-one with `VESICA_LOG_LEVEL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        })
    }
}

/// stdout log rendering: plain text (human-readable, journald-friendly) or
/// newline-delimited JSON (machine-parseable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

/// A secret value that is never printed. `Debug` and `Display` both render a
/// fixed placeholder regardless of content; call [`Redacted::expose_secret`]
/// to get the real value where it is actually needed (e.g. an `Authorization`
/// header check).
#[derive(Clone, PartialEq, Eq)]
pub struct Redacted(String);

impl Redacted {
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Redacted(**REDACTED**)")
    }
}

impl fmt::Display for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("**REDACTED**")
    }
}

/// Fully validated runtime configuration. See the module docs for how it is
/// loaded.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: std::net::IpAddr,
    pub port: u16,
    pub public_url: String,
    pub db_path: PathBuf,
    pub blob_dir: PathBuf,
    pub max_upload_mb: u32,
    pub auth_mode: AuthMode,
    pub api_token: Redacted,
    pub mcp_http_enabled: bool,
    pub er_suggest_threshold: f64,
    pub er_strong_threshold: f64,
    pub er_cross_case: bool,
    pub log_level: LogLevel,
    pub log_format: LogFormat,
    pub backup_enabled: bool,
    pub backup_dir: PathBuf,
    pub backup_keep: u32,
}

impl Config {
    /// Loads configuration from the real process environment layered over
    /// `./.env` (if present) layered over defaults, validating as it goes.
    pub fn load() -> anyhow::Result<Config> {
        let env_entries: Vec<(String, String)> = std::env::vars().collect();
        let dotenv_content = std::fs::read_to_string(Path::new(".env")).ok();
        Config::from_sources(&env_entries, dotenv_content.as_deref())
    }

    /// Builds and validates configuration from explicit sources, bypassing
    /// the real process environment and filesystem. This is what makes the
    /// config loader unit-testable without racing other tests over shared
    /// global process environment state.
    pub fn from_sources(
        env_entries: &[(String, String)],
        dotenv_content: Option<&str>,
    ) -> anyhow::Result<Config> {
        let mut figment = Figment::new().merge(KeyedStrings::new(
            "built-in defaults",
            DEFAULTS
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        ));

        if let Some(content) = dotenv_content {
            let pairs = strip_prefix_lowercase(parse_dotenv(content), ENV_PREFIX);
            figment = figment.merge(KeyedStrings::new(".env file", pairs));
        }

        let pairs = strip_prefix_lowercase(env_entries.iter().cloned(), ENV_PREFIX);
        figment = figment.merge(KeyedStrings::new("process environment", pairs));

        let raw = RawConfig::extract(&figment)?;
        raw.validate()
    }
}

/// Every field, still a raw string pulled out of the merged `Figment`, ready
/// for typed parsing and validation.
struct RawConfig {
    bind_addr: String,
    port: String,
    public_url: String,
    db_path: String,
    blob_dir: String,
    max_upload_mb: String,
    auth_mode: String,
    api_token: String,
    mcp_http_enabled: String,
    er_suggest_threshold: String,
    er_strong_threshold: String,
    er_cross_case: String,
    log_level: String,
    log_format: String,
    backup_enabled: String,
    backup_dir: String,
    backup_keep: String,
}

impl RawConfig {
    /// Every source (defaults, `.env`, environment) is stored as plain
    /// strings, so extraction can never hit a type-coercion surprise (e.g.
    /// an all-digit secret token being parsed as a number) — every key is
    /// guaranteed present because `DEFAULTS` covers all of them.
    fn extract(figment: &Figment) -> anyhow::Result<RawConfig> {
        let get = |key: &str| -> anyhow::Result<String> {
            figment.extract_inner::<String>(key).map_err(|e| {
                anyhow::anyhow!(
                    "{}{}: failed to read configuration value: {e}",
                    ENV_PREFIX,
                    key.to_uppercase()
                )
            })
        };
        Ok(RawConfig {
            bind_addr: get("bind_addr")?,
            port: get("port")?,
            public_url: get("public_url")?,
            db_path: get("db_path")?,
            blob_dir: get("blob_dir")?,
            max_upload_mb: get("max_upload_mb")?,
            auth_mode: get("auth_mode")?,
            api_token: get("api_token")?,
            mcp_http_enabled: get("mcp_http_enabled")?,
            er_suggest_threshold: get("er_suggest_threshold")?,
            er_strong_threshold: get("er_strong_threshold")?,
            er_cross_case: get("er_cross_case")?,
            log_level: get("log_level")?,
            log_format: get("log_format")?,
            backup_enabled: get("backup_enabled")?,
            backup_dir: get("backup_dir")?,
            backup_keep: get("backup_keep")?,
        })
    }

    fn validate(self) -> anyhow::Result<Config> {
        let mut errors: Vec<String> = Vec::new();

        let bind_addr = parse_field(
            &mut errors,
            "bind_addr",
            &self.bind_addr,
            "a valid IPv4 or IPv6 address, e.g. 127.0.0.1 or 0.0.0.0",
            |s| s.parse::<std::net::IpAddr>().ok(),
        )
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));

        let port = parse_field(
            &mut errors,
            "port",
            &self.port,
            "an integer between 1 and 65535",
            |s| {
                s.parse::<u32>()
                    .ok()
                    .filter(|p| (1..=65535).contains(p))
                    .map(|p| p as u16)
            },
        )
        .unwrap_or(8787);

        let public_url = if self.public_url.trim().is_empty()
            || !(self.public_url.starts_with("http://") || self.public_url.starts_with("https://"))
            || self.public_url.trim_end_matches('/').len() <= "https://".len()
        {
            push_error(
                &mut errors,
                "public_url",
                &format!("'{}' is not a usable URL", self.public_url),
                "an http(s) URL with a host, e.g. http://127.0.0.1:8787",
            );
            self.public_url.clone()
        } else {
            self.public_url.clone()
        };

        let db_path = validate_path(&mut errors, "db_path", &self.db_path, PathKind::File);
        let blob_dir = validate_path(&mut errors, "blob_dir", &self.blob_dir, PathKind::Dir);
        let backup_dir = validate_path(&mut errors, "backup_dir", &self.backup_dir, PathKind::Dir);

        let max_upload_mb = parse_field(
            &mut errors,
            "max_upload_mb",
            &self.max_upload_mb,
            "an integer between 1 and 10240 (megabytes, up to 10 GiB)",
            |s| s.parse::<u32>().ok().filter(|v| (1..=10240).contains(v)),
        )
        .unwrap_or(256);

        let auth_mode = parse_field(
            &mut errors,
            "auth_mode",
            &self.auth_mode,
            "one of: solo, token",
            |s| match s.to_ascii_lowercase().as_str() {
                "solo" => Some(AuthMode::Solo),
                "token" => Some(AuthMode::Token),
                _ => None,
            },
        )
        .unwrap_or(AuthMode::Solo);

        let api_token = Redacted(self.api_token.clone());
        if auth_mode == AuthMode::Token && api_token.is_empty() {
            push_error(
                &mut errors,
                "api_token",
                "must not be empty when VESICA_AUTH_MODE=token",
                "a non-empty shared secret; generate one with: openssl rand -hex 32",
            );
        }

        let mcp_http_enabled =
            parse_bool_field(&mut errors, "mcp_http_enabled", &self.mcp_http_enabled)
                .unwrap_or(true);
        let er_cross_case =
            parse_bool_field(&mut errors, "er_cross_case", &self.er_cross_case).unwrap_or(true);
        let backup_enabled =
            parse_bool_field(&mut errors, "backup_enabled", &self.backup_enabled).unwrap_or(true);

        let er_suggest_threshold = parse_field(
            &mut errors,
            "er_suggest_threshold",
            &self.er_suggest_threshold,
            "a decimal between 0.0 and 1.0",
            |s| s.parse::<f64>().ok().filter(|v| (0.0..=1.0).contains(v)),
        )
        .unwrap_or(0.62);

        let er_strong_threshold = parse_field(
            &mut errors,
            "er_strong_threshold",
            &self.er_strong_threshold,
            "a decimal between 0.0 and 1.0",
            |s| s.parse::<f64>().ok().filter(|v| (0.0..=1.0).contains(v)),
        )
        .unwrap_or(0.85);

        if er_strong_threshold < er_suggest_threshold {
            push_error(
                &mut errors,
                "er_strong_threshold",
                &format!(
                    "{er_strong_threshold} is lower than VESICA_ER_SUGGEST_THRESHOLD ({er_suggest_threshold})"
                ),
                &format!("a decimal >= {er_suggest_threshold} (the suggest threshold) and <= 1.0"),
            );
        }

        let log_level = parse_field(
            &mut errors,
            "log_level",
            &self.log_level,
            "one of: trace, debug, info, warn, error",
            |s| match s.to_ascii_lowercase().as_str() {
                "trace" => Some(LogLevel::Trace),
                "debug" => Some(LogLevel::Debug),
                "info" => Some(LogLevel::Info),
                "warn" => Some(LogLevel::Warn),
                "error" => Some(LogLevel::Error),
                _ => None,
            },
        )
        .unwrap_or(LogLevel::Info);

        let log_format = parse_field(
            &mut errors,
            "log_format",
            &self.log_format,
            "one of: text, json",
            |s| match s.to_ascii_lowercase().as_str() {
                "text" => Some(LogFormat::Text),
                "json" => Some(LogFormat::Json),
                _ => None,
            },
        )
        .unwrap_or(LogFormat::Text);

        let backup_keep = parse_field(
            &mut errors,
            "backup_keep",
            &self.backup_keep,
            "an integer between 1 and 3650 (nightly backups to retain)",
            |s| s.parse::<u32>().ok().filter(|v| (1..=3650).contains(v)),
        )
        .unwrap_or(14);

        if !errors.is_empty() {
            anyhow::bail!("invalid configuration:\n{}", errors.join("\n"));
        }

        Ok(Config {
            bind_addr,
            port,
            public_url,
            db_path,
            blob_dir,
            max_upload_mb,
            auth_mode,
            api_token,
            mcp_http_enabled,
            er_suggest_threshold,
            er_strong_threshold,
            er_cross_case,
            log_level,
            log_format,
            backup_enabled,
            backup_dir,
            backup_keep,
        })
    }
}

enum PathKind {
    File,
    Dir,
}

fn validate_path(errors: &mut Vec<String>, key: &str, raw: &str, kind: PathKind) -> PathBuf {
    if raw.trim().is_empty() {
        push_error(
            errors,
            key,
            "must not be empty",
            "a filesystem path, e.g. ./data/vesica.sqlite",
        );
        return PathBuf::new();
    }
    let path = PathBuf::from(raw);
    match kind {
        PathKind::File => {
            if path.is_dir() {
                push_error(
                    errors,
                    key,
                    &format!("'{raw}' already exists and is a directory"),
                    "a path to a file, not a directory",
                );
            }
        }
        PathKind::Dir => {
            if path.is_file() {
                push_error(
                    errors,
                    key,
                    &format!("'{raw}' already exists and is a file"),
                    "a path to a directory, not a file",
                );
            }
        }
    }
    path
}

fn push_error(errors: &mut Vec<String>, key: &str, message: &str, expected: &str) {
    errors.push(format!(
        "{}{}: {message} (expected {expected})",
        ENV_PREFIX,
        key.to_ascii_uppercase()
    ));
}

/// Parses `raw` with `f`; on failure records a validation error naming the
/// key, what was given and what was expected, and returns `None`.
fn parse_field<T>(
    errors: &mut Vec<String>,
    key: &str,
    raw: &str,
    expected: &str,
    f: impl FnOnce(&str) -> Option<T>,
) -> Option<T> {
    match f(raw) {
        Some(v) => Some(v),
        None => {
            push_error(errors, key, &format!("'{raw}' is not valid"), expected);
            None
        }
    }
}

fn parse_bool_field(errors: &mut Vec<String>, key: &str, raw: &str) -> Option<bool> {
    parse_field(errors, key, raw, "true or false", |s| {
        match s.to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    })
}

/// Parses a simple `KEY=VALUE` `.env` file: blank lines and full-line `#`
/// comments are skipped, and a value may optionally be wrapped in matching
/// single or double quotes. Not a general shell-dotenv parser — deliberately
/// only as much as `env.example.txt`'s own format needs.
fn parse_dotenv(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim().to_string();
            let mut value = value.trim();
            if value.len() >= 2 {
                let bytes = value.as_bytes();
                let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
                if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                    value = &value[1..value.len() - 1];
                }
            }
            Some((key, value.to_string()))
        })
        .collect()
}

/// Strips `prefix` (case-insensitively) from each key and lowercases the
/// remainder; pairs whose key doesn't carry the prefix are dropped (they
/// belong to some other application, not Vesica).
fn strip_prefix_lowercase(
    pairs: impl IntoIterator<Item = (String, String)>,
    prefix: &str,
) -> Vec<(String, String)> {
    pairs
        .into_iter()
        .filter_map(|(k, v)| {
            let upper = k.to_ascii_uppercase();
            upper
                .strip_prefix(prefix)
                .map(|rest| (rest.to_ascii_lowercase(), v))
        })
        .collect()
}

/// A `figment::Provider` over a flat list of already-resolved string
/// key/value pairs. Every value is stored as [`Value::String`] — never
/// coerced to a bool or number at this layer — so extraction and validation
/// stay entirely under our own control instead of `figment`'s TOML-like
/// value-sniffing (which would otherwise risk, for example, an all-digit
/// secret token being silently parsed as a number).
struct KeyedStrings {
    name: &'static str,
    entries: Vec<(String, String)>,
}

impl KeyedStrings {
    fn new(name: &'static str, entries: impl IntoIterator<Item = (String, String)>) -> Self {
        KeyedStrings {
            name,
            entries: entries.into_iter().collect(),
        }
    }
}

impl Provider for KeyedStrings {
    fn metadata(&self) -> Metadata {
        Metadata::named(self.name)
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let mut dict = Dict::new();
        for (k, v) in &self.entries {
            dict.insert(k.clone(), Value::from(v.as_str()));
        }
        let mut map = Map::new();
        map.insert(Profile::Default, dict);
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `.env.example` (checked out at the workspace root) and `DEFAULTS`
    /// must name exactly the same set of `VESICA_*` keys, in both
    /// directions, so the two representations cannot silently diverge.
    #[test]
    fn all_env_example_keys_map_to_config_fields() {
        let content = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.example"));
        let file_keys: Vec<String> = content
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                line.split_once('=').map(|(k, _)| k.trim().to_string())
            })
            .collect();
        assert!(
            !file_keys.is_empty(),
            ".env.example should list at least one key"
        );

        for key in &file_keys {
            let stripped = key.strip_prefix(ENV_PREFIX).unwrap_or_else(|| {
                panic!(".env.example key '{key}' is missing the VESICA_ prefix")
            });
            let field = stripped.to_ascii_lowercase();
            assert!(
                DEFAULTS.iter().any(|(k, _)| *k == field),
                "'{key}' in .env.example has no matching Config field/default"
            );
        }

        for (field, _) in DEFAULTS {
            let expected_key = format!("{ENV_PREFIX}{}", field.to_ascii_uppercase());
            assert!(
                file_keys.contains(&expected_key),
                "Config field '{field}' has no '{expected_key}' entry in .env.example"
            );
        }
    }

    #[test]
    fn missing_dotenv_is_not_an_error_and_defaults_apply() {
        let config = Config::from_sources(&[], None).expect("defaults alone must be valid");
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1");
        assert_eq!(config.port, 8787);
        assert_eq!(config.auth_mode, AuthMode::Solo);
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.log_format, LogFormat::Text);
        assert!(config.mcp_http_enabled);
        assert!(config.er_cross_case);
        assert!(config.backup_enabled);
        assert_eq!(config.backup_keep, 14);
    }

    #[test]
    fn dotenv_layers_over_defaults_and_env_layers_over_dotenv() {
        let dotenv = "VESICA_PORT=9999\nVESICA_LOG_LEVEL=debug\n";
        let env = [("VESICA_PORT".to_string(), "7000".to_string())];
        let config = Config::from_sources(&env, Some(dotenv)).unwrap();
        // Real env var wins over .env for the key both set.
        assert_eq!(config.port, 7000);
        // .env applies for a key the real environment didn't set.
        assert_eq!(config.log_level, LogLevel::Debug);
    }

    #[test]
    fn dotenv_parses_comments_blank_lines_and_quotes() {
        let content = "\n# a comment\nVESICA_BIND_ADDR=0.0.0.0\nVESICA_PUBLIC_URL=\"http://example.test\"\n  \nVESICA_LOG_LEVEL='warn'\n";
        let pairs = parse_dotenv(content);
        assert_eq!(pairs.len(), 3);
        assert!(pairs.contains(&("VESICA_BIND_ADDR".to_string(), "0.0.0.0".to_string())));
        assert!(pairs.contains(&(
            "VESICA_PUBLIC_URL".to_string(),
            "http://example.test".to_string()
        )));
        assert!(pairs.contains(&("VESICA_LOG_LEVEL".to_string(), "warn".to_string())));
    }

    #[test]
    fn bad_port_is_rejected_with_key_and_expectation() {
        let env = [("VESICA_PORT".to_string(), "99999".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(
            err.contains("VESICA_PORT"),
            "error should name the key: {err}"
        );
        assert!(
            err.contains("65535"),
            "error should say what was expected: {err}"
        );
    }

    #[test]
    fn non_numeric_port_is_rejected() {
        let env = [("VESICA_PORT".to_string(), "not-a-port".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_PORT"));
    }

    #[test]
    fn bad_bind_addr_is_rejected() {
        let env = [("VESICA_BIND_ADDR".to_string(), "not-an-ip".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_BIND_ADDR"));
    }

    #[test]
    fn token_auth_mode_requires_non_empty_token() {
        let env = [("VESICA_AUTH_MODE".to_string(), "token".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(
            err.contains("VESICA_API_TOKEN"),
            "error should name the key: {err}"
        );
        assert!(
            err.contains("empty"),
            "error should explain what's wrong: {err}"
        );
    }

    #[test]
    fn token_auth_mode_with_token_succeeds() {
        let env = [
            ("VESICA_AUTH_MODE".to_string(), "token".to_string()),
            ("VESICA_API_TOKEN".to_string(), "a-secret-value".to_string()),
        ];
        let config = Config::from_sources(&env, None).unwrap();
        assert_eq!(config.auth_mode, AuthMode::Token);
        assert_eq!(config.api_token.expose_secret(), "a-secret-value");
    }

    #[test]
    fn invalid_auth_mode_is_rejected() {
        let env = [("VESICA_AUTH_MODE".to_string(), "admin".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_AUTH_MODE"));
        assert!(err.contains("solo"));
        assert!(err.contains("token"));
    }

    #[test]
    fn strong_threshold_below_suggest_threshold_is_rejected() {
        let env = [
            ("VESICA_ER_SUGGEST_THRESHOLD".to_string(), "0.7".to_string()),
            ("VESICA_ER_STRONG_THRESHOLD".to_string(), "0.5".to_string()),
        ];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(
            err.contains("VESICA_ER_STRONG_THRESHOLD"),
            "error should name the key: {err}"
        );
    }

    #[test]
    fn threshold_out_of_unit_range_is_rejected() {
        let env = [("VESICA_ER_SUGGEST_THRESHOLD".to_string(), "1.5".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_ER_SUGGEST_THRESHOLD"));
    }

    #[test]
    fn invalid_log_level_is_rejected() {
        let env = [("VESICA_LOG_LEVEL".to_string(), "verbose".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_LOG_LEVEL"));
    }

    #[test]
    fn invalid_bool_field_is_rejected() {
        let env = [(
            "VESICA_MCP_HTTP_ENABLED".to_string(),
            "yes-please".to_string(),
        )];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_MCP_HTTP_ENABLED"));
    }

    #[test]
    fn max_upload_mb_out_of_range_is_rejected() {
        let env = [("VESICA_MAX_UPLOAD_MB".to_string(), "0".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_MAX_UPLOAD_MB"));
    }

    #[test]
    fn backup_keep_out_of_range_is_rejected() {
        let env = [("VESICA_BACKUP_KEEP".to_string(), "0".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_BACKUP_KEEP"));
    }

    #[test]
    fn invalid_public_url_is_rejected() {
        let env = [("VESICA_PUBLIC_URL".to_string(), "not-a-url".to_string())];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_PUBLIC_URL"));
    }

    #[test]
    fn multiple_errors_are_all_reported_together() {
        let env = [
            ("VESICA_PORT".to_string(), "0".to_string()),
            ("VESICA_LOG_LEVEL".to_string(), "bogus".to_string()),
        ];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_PORT"), "{err}");
        assert!(err.contains("VESICA_LOG_LEVEL"), "{err}");
    }

    /// The whole point of `Redacted`: a `Config` carrying a real secret must
    /// never leak it through `{:?}`.
    #[test]
    fn debug_rendering_never_contains_the_raw_token() {
        let secret = "sk-super-duper-secret-token-value-12345";
        let env = [
            ("VESICA_AUTH_MODE".to_string(), "token".to_string()),
            ("VESICA_API_TOKEN".to_string(), secret.to_string()),
        ];
        let config = Config::from_sources(&env, None).unwrap();
        let rendered = format!("{config:?}");
        assert!(
            !rendered.contains(secret),
            "Debug output leaked the token: {rendered}"
        );
        assert!(
            rendered.contains("REDACTED"),
            "Debug output should show a redaction marker: {rendered}"
        );

        let token_only = format!("{:?}", config.api_token);
        assert!(!token_only.contains(secret));
        let display_only = format!("{}", config.api_token);
        assert!(!display_only.contains(secret));
    }

    #[test]
    fn path_that_exists_as_wrong_kind_is_rejected() {
        let dir = std::env::temp_dir().join(format!("vesica-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let env = [(
            "VESICA_DB_PATH".to_string(),
            dir.to_string_lossy().into_owned(),
        )];
        let err = Config::from_sources(&env, None).unwrap_err().to_string();
        assert!(err.contains("VESICA_DB_PATH"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
