//! `email`: address folding, with provider-specific local-part rules.
//!
//! Two rules, and the difference between them matters. `+tag` suffixes are
//! stripped everywhere, because sub-addressing is near-universal and
//! `jan+lek@example.com` really is `jan@example.com`. Dots in the local part
//! are folded **only** for Google's domains, because that is the only large
//! provider for which `j.kowalski@` and `jkowalski@` are the same mailbox.
//! Applying Gmail's rule to every domain would merge two different people at
//! any provider that treats dots as significant.

use crate::property::{
    AUX_DOMAIN, AUX_TAG, BlockingKey, BlockingKeyKind, ErrorCode, Normalized, PropertyType,
    ValidationError,
};

use super::weburl::fold_host;

const FIX: &str = "supply an address of the form `local@domain.tld`, e.g. \
                   `jan.kowalski@example.com`";

/// Domains whose local part is dot-insensitive. Deliberately short: this list
/// grows only when a provider is confirmed to fold dots.
const DOT_FOLDING_DOMAINS: &[&str] = &["gmail.com", "googlemail.com"];

/// Domains that are aliases of another domain's mailbox namespace.
const DOMAIN_ALIASES: &[(&str, &str)] = &[("googlemail.com", "gmail.com")];

fn err(code: ErrorCode, message: &str) -> ValidationError {
    ValidationError::new(PropertyType::Email, code, message, FIX)
}

pub(super) fn normalize(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Email, FIX));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(err(
            ErrorCode::Malformed,
            "an email address may not contain whitespace",
        ));
    }

    // Split at the last `@`: a quoted local part may legally contain one.
    let Some(at) = trimmed.rfind('@') else {
        return Err(err(ErrorCode::Malformed, "no `@` in the address"));
    };
    let (local_raw, domain_raw) = trimmed.split_at(at);
    let domain_raw = &domain_raw[1..];

    if local_raw.is_empty() {
        return Err(err(ErrorCode::Malformed, "the part before `@` is empty"));
    }
    if domain_raw.is_empty() {
        return Err(err(ErrorCode::Malformed, "the part after `@` is empty"));
    }

    let domain = fold_host(domain_raw).ok_or_else(|| {
        err(
            ErrorCode::Malformed,
            "the part after `@` is not a valid domain",
        )
    })?;
    if !domain.contains('.') {
        return Err(err(
            ErrorCode::Malformed,
            "the domain has no dot, so it is not a routable address",
        ));
    }

    let domain = DOMAIN_ALIASES
        .iter()
        .find(|(from, _)| *from == domain)
        .map_or(domain.clone(), |(_, to)| (*to).to_string());

    // A quoted local part is left exactly as it is: inside quotes, dots,
    // pluses and case are all significant.
    let quoted = local_raw.starts_with('"') && local_raw.ends_with('"') && local_raw.len() >= 2;
    let (local, tag) = if quoted {
        (local_raw.to_string(), String::new())
    } else {
        fold_local(local_raw, &domain)
    };

    if local.is_empty() {
        return Err(err(
            ErrorCode::Malformed,
            "the part before `@` is empty once the `+tag` is removed",
        ));
    }

    Ok(
        Normalized::new(PropertyType::Email, raw, format!("{local}@{domain}"))
            .with_aux(AUX_DOMAIN, domain)
            .with_aux(AUX_TAG, tag),
    )
}

/// Returns the folded local part and the `+tag` that was removed, if any.
fn fold_local(local_raw: &str, domain: &str) -> (String, String) {
    let lowered = local_raw.to_lowercase();
    // A leading `+` is part of the address, not a tag separator, so the search
    // skips the first character. `char_indices` keeps the split on a character
    // boundary for a local part that is not ASCII.
    let plus = lowered
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+')
        .map(|(i, _)| i);
    let (base, tag) = match plus {
        Some(cut) => (lowered[..cut].to_string(), lowered[cut + 1..].to_string()),
        None => (lowered, String::new()),
    };
    let base = if DOT_FOLDING_DOMAINS.contains(&domain) {
        base.replace('.', "")
    } else {
        base
    };
    (base, tag)
}

pub(super) fn blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    // The folded address only. The domain alone is not a blocking key: it
    // would put every gmail user in one bucket.
    vec![BlockingKey::new(BlockingKeyKind::Email, n.value.clone())]
}
