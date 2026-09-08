//! `url` and `domain`.
//!
//! URLs fold scheme and host to lower case, drop a default port, drop the
//! fragment and drop tracking parameters. The path keeps its case, because
//! paths are case-sensitive on most servers and folding them would merge two
//! genuinely different pages.
//!
//! Domains are punycode-aware in both directions: an internationalised name is
//! stored in its ASCII form so that `bücher.de` and `xn--bcher-kva.de` are one
//! value, and the registrable domain is kept alongside it so that
//! `mail.example.co.uk` blocks against `www.example.co.uk`.

use url::{Host, Url};

use crate::property::{
    AUX_DOMAIN, AUX_REGISTRABLE, BlockingKey, BlockingKeyKind, ErrorCode, Normalized, PropertyType,
    ValidationError,
};
use crate::warning::{Warning, WarningCode};

const URL_FIX: &str = "supply an absolute URL, e.g. `https://example.com/page`";
const DOMAIN_FIX: &str = "supply a bare domain name, e.g. `example.com` — no scheme, \
                          no path, no `@`";

/// Query parameters that identify a campaign or a click rather than a
/// resource. Exact names; see [`is_tracking_param`] for the prefix rules.
const TRACKING_PARAMS: &[&str] = &[
    "gclid",
    "gclsrc",
    "gbraid",
    "wbraid",
    "dclid",
    "fbclid",
    "msclkid",
    "twclid",
    "ttclid",
    "yclid",
    "igshid",
    "igsh",
    "mc_cid",
    "mc_eid",
    "mkt_tok",
    "vero_id",
    "vero_conv",
    "oly_anon_id",
    "oly_enc_id",
    "s_kwcid",
    "ref_src",
    "ref_url",
    "icid",
    "cmpid",
    "_ga",
    "_gl",
    "spm",
    "scm",
];

/// Prefixes shared by whole families of tracking parameters.
const TRACKING_PREFIXES: &[&str] = &["utm_", "pk_", "piwik_", "matomo_", "hsa_", "_hs"];

fn is_tracking_param(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    TRACKING_PARAMS.contains(&lower.as_str())
        || TRACKING_PREFIXES.iter().any(|p| lower.starts_with(p))
}

/// Fold a bare host: strip a trailing dot, lower-case it, and convert an
/// internationalised name to punycode. `None` if it is not a host at all.
pub(super) fn fold_host(host: &str) -> Option<String> {
    let trimmed = host.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed.contains(['/', '@', '\\', ' ', '?', '#']) {
        return None;
    }
    match Host::parse(trimmed).ok()? {
        Host::Domain(d) => {
            let d = d.to_ascii_lowercase();
            if d.is_empty() { None } else { Some(d) }
        }
        // A bracketed IPv6 literal or a dotted-quad is a legitimate host.
        other => Some(other.to_string().to_ascii_lowercase()),
    }
}

/// The registrable domain (`example.co.uk` from `mail.example.co.uk`) using
/// the Public Suffix List. `None` for hosts with no registrable form, such as
/// `localhost` or a bare IP.
fn registrable(host: &str) -> Option<String> {
    psl::domain_str(host).map(str::to_ascii_lowercase)
}

pub(super) fn normalize_url(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Url, URL_FIX));
    }

    let mut warnings = Vec::new();
    let mut parsed = match Url::parse(trimmed) {
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            // `example.com/page` is what people paste. Assume https and say so.
            warnings.push(Warning::new(
                WarningCode::UrlSchemeAssumed,
                format!("`{trimmed}` had no scheme; read as https"),
            ));
            Url::parse(&format!("https://{trimmed}")).map_err(|e| {
                ValidationError::new(
                    PropertyType::Url,
                    ErrorCode::Malformed,
                    format!("not a URL: {e}"),
                    URL_FIX,
                )
            })?
        }
        Err(e) => {
            return Err(ValidationError::new(
                PropertyType::Url,
                ErrorCode::Malformed,
                format!("not a URL: {e}"),
                URL_FIX,
            ));
        }
    };

    // `Url` already lower-cases the scheme, lower-cases and punycodes the
    // host, and removes a default port.
    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    {
        let mut q = parsed.query_pairs_mut();
        q.clear();
        for (k, v) in &kept {
            q.append_pair(k, v);
        }
    }
    if kept.is_empty() {
        parsed.set_query(None);
    }
    // A fragment addresses a position inside a document, not a document.
    parsed.set_fragment(None);

    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let mut value = parsed.to_string();
    // `https://example.com/` and `https://example.com` are the same resource.
    if parsed.path() == "/" && parsed.query().is_none() && value.ends_with('/') {
        value.pop();
    }

    Ok(Normalized::new(PropertyType::Url, raw, value)
        .with_aux(AUX_DOMAIN, host.clone())
        .with_aux(AUX_REGISTRABLE, registrable(&host).unwrap_or_default())
        .with_warnings(warnings))
}

pub(super) fn normalize_domain(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Domain, DOMAIN_FIX));
    }
    let host = fold_host(trimmed).ok_or_else(|| {
        ValidationError::new(
            PropertyType::Domain,
            ErrorCode::Malformed,
            format!("`{trimmed}` is not a domain name"),
            DOMAIN_FIX,
        )
    })?;

    Ok(Normalized::new(PropertyType::Domain, raw, host.clone())
        .with_aux(AUX_DOMAIN, host.clone())
        .with_aux(AUX_REGISTRABLE, registrable(&host).unwrap_or_default()))
}

pub(super) fn blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    let mut keys = Vec::new();
    if let Some(domain) = n.aux(AUX_DOMAIN) {
        keys.push(BlockingKey::new(BlockingKeyKind::Domain, domain));
    }
    if let Some(reg) = n.aux(AUX_REGISTRABLE) {
        keys.push(BlockingKey::new(BlockingKeyKind::Domain, reg));
    }
    keys
}
