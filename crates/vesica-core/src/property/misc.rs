//! The types whose normalisation is a single rule: `ip`, `date`, `country`,
//! `text`, `number` and `entity`.

use std::net::IpAddr;
use std::str::FromStr;

use crate::date::PartialDate;
use crate::property::{
    BlockingKey, BlockingKeyKind, ErrorCode, NormalizeContext, Normalized, PropertyType,
    ValidationError,
};

const IP_FIX: &str = "supply an IPv4 or IPv6 address, e.g. `192.0.2.4` or `2001:db8::1`";
const COUNTRY_FIX: &str = "supply an ISO-3166-1 alpha-2 code such as `PL`, or the English \
                           country name such as `Poland`";
const NUMBER_FIX: &str = "supply a number, e.g. `42.5`; a comma decimal separator and \
                          space thousands separators are accepted";
const ENTITY_FIX: &str = "supply the 26-character ULID of an existing entity";

/// `ip`: canonical form, which for IPv6 means lower-case and maximally
/// compressed. `::ffff:192.0.2.1` and `[::1]` are the same addresses as their
/// bare forms, so the brackets go and the mapping is preserved.
pub(super) fn normalize_ip(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Ip, IP_FIX));
    }
    let addr = IpAddr::from_str(trimmed).map_err(|_| {
        ValidationError::new(
            PropertyType::Ip,
            ErrorCode::Malformed,
            format!("`{trimmed}` is not an IP address"),
            IP_FIX,
        )
    })?;
    Ok(Normalized::new(
        PropertyType::Ip,
        raw,
        addr.to_string().to_ascii_lowercase(),
    ))
}

/// `date`: a [`PartialDate`], never a point in time.
pub(super) fn normalize_date(
    raw: &str,
    ctx: &NormalizeContext,
) -> Result<Normalized, ValidationError> {
    let parsed = PartialDate::parse(raw, ctx.date_style()).map_err(|e| {
        let code = match e {
            crate::date::DateError::Empty => ErrorCode::Empty,
            crate::date::DateError::TwoDigitYear { .. } => ErrorCode::Ambiguous,
            crate::date::DateError::MonthOutOfRange { .. }
            | crate::date::DateError::DayOutOfRange { .. }
            | crate::date::DateError::YearOutOfRange { .. } => ErrorCode::OutOfRange,
            _ => ErrorCode::Malformed,
        };
        ValidationError::new(PropertyType::Date, code, e.to_string(), e.fix())
    })?;

    let mut out =
        Normalized::new(PropertyType::Date, raw, parsed.date.text()).with_warnings(parsed.warnings);
    out.date = Some(parsed.date);
    Ok(out)
}

pub(super) fn date_blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    // One key per precision level, coarsest first, so that a date known only
    // to the year still blocks against the same date known to the day.
    n.date
        .as_ref()
        .map(|d| {
            d.iso_prefixes()
                .into_iter()
                .map(|p| BlockingKey::new(BlockingKeyKind::Dob, p))
                .collect()
        })
        .unwrap_or_default()
}

/// Endonyms and everyday short forms that the ISO English short-name table
/// does not carry. Kept short on purpose: this is a convenience for typing,
/// not a gazetteer.
const COUNTRY_ALIASES: &[(&str, &str)] = &[
    ("polska", "PL"),
    ("rzeczpospolita polska", "PL"),
    ("deutschland", "DE"),
    ("germany", "DE"),
    ("russia", "RU"),
    ("uk", "GB"),
    ("great britain", "GB"),
    ("united kingdom", "GB"),
    ("england", "GB"),
    ("scotland", "GB"),
    ("wales", "GB"),
    ("northern ireland", "GB"),
    ("usa", "US"),
    ("u s a", "US"),
    ("united states", "US"),
    ("united states of america", "US"),
    ("america", "US"),
    ("south korea", "KR"),
    ("north korea", "KP"),
    ("czech republic", "CZ"),
    ("czechia", "CZ"),
    ("holland", "NL"),
    ("netherlands", "NL"),
    ("vietnam", "VN"),
    ("ivory coast", "CI"),
    ("cape verde", "CV"),
    ("swaziland", "SZ"),
    ("burma", "MM"),
    ("uae", "AE"),
    ("united arab emirates", "AE"),
    ("laos", "LA"),
    ("syria", "SY"),
    ("iran", "IR"),
    ("bolivia", "BO"),
    ("venezuela", "VE"),
    ("tanzania", "TZ"),
    ("moldova", "MD"),
    ("macedonia", "MK"),
    ("north macedonia", "MK"),
    ("turkey", "TR"),
    ("belarus", "BY"),
    ("ukraine", "UA"),
];

/// `country`: ISO-3166-1 alpha-2.
///
/// Alpha-2, alpha-3, numeric and the English short name are all accepted; the
/// stored form is always alpha-2. A country this table does not know is an
/// error rather than a warning, because the normalisation contract for this
/// type *is* "an alpha-2 code" — there is no degraded form to store. The `fix`
/// string tells an agent exactly what to send instead, and `text` is available
/// for anything genuinely outside ISO-3166.
pub(super) fn normalize_country(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Country, COUNTRY_FIX));
    }
    let folded = crate::fold::fold_for_comparison(trimmed);
    let upper = trimmed.to_ascii_uppercase();

    let code = rust_iso3166::from_alpha2(&upper)
        .or_else(|| rust_iso3166::from_alpha3(&upper))
        .or_else(|| rust_iso3166::from_numeric_str(&upper))
        .map(|c| c.alpha2.to_string())
        .or_else(|| {
            COUNTRY_ALIASES
                .iter()
                .find(|(name, _)| *name == folded)
                .map(|(_, code)| (*code).to_string())
        })
        .or_else(|| {
            rust_iso3166::ALL
                .iter()
                .find(|c| crate::fold::fold_for_comparison(c.name) == folded)
                .map(|c| c.alpha2.to_string())
        })
        .ok_or_else(|| {
            ValidationError::new(
                PropertyType::Country,
                ErrorCode::UnknownValue,
                format!("`{trimmed}` is not a country this build recognises"),
                COUNTRY_FIX,
            )
        })?;

    Ok(Normalized::new(PropertyType::Country, raw, code))
}

/// `text`: trim only. Free prose is never folded, because it is read by people
/// and never matched on.
pub(super) fn normalize_text(raw: &str) -> Result<Normalized, ValidationError> {
    Ok(Normalized::new(PropertyType::Text, raw, raw.trim()))
}

/// `number`: a decimal, tolerant of the separators a European keyboard
/// produces.
pub(super) fn normalize_number(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Number, NUMBER_FIX));
    }
    // Space (including the non-breaking and narrow no-break spaces a
    // spreadsheet emits) groups thousands; a lone comma is a decimal point.
    let stripped: String = trimmed
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{00a0}' && *c != '\u{202f}')
        .collect();
    // A single comma is a decimal point unless it is followed by exactly three
    // digits with something before it, which is the shape of a thousands
    // group. `42,5` is forty-two and a half; `1,234` is one thousand.
    let single_comma_decimal = stripped.matches(',').count() == 1
        && !stripped.contains('.')
        && match stripped.split_once(',') {
            Some((before, after)) => {
                before.chars().any(|c| c.is_ascii_digit())
                    && !(after.len() == 3 && after.chars().all(|c| c.is_ascii_digit()))
            }
            None => false,
        };
    let candidate = if single_comma_decimal {
        stripped.replace(',', ".")
    } else {
        stripped.replace(',', "")
    };

    let n: f64 = candidate.parse().map_err(|_| {
        ValidationError::new(
            PropertyType::Number,
            ErrorCode::Malformed,
            format!("`{trimmed}` is not a number"),
            NUMBER_FIX,
        )
    })?;
    if !n.is_finite() {
        return Err(ValidationError::new(
            PropertyType::Number,
            ErrorCode::OutOfRange,
            format!("`{trimmed}` is not a finite number"),
            NUMBER_FIX,
        ));
    }

    let mut out = Normalized::new(PropertyType::Number, raw, format_number(n));
    out.numeric = Some(n);
    Ok(out)
}

/// Render a float without an exponent or a trailing `.0`, so that `42`, `42.0`
/// and ` 42 ` share one normalised form and re-normalising it is a no-op.
fn format_number(n: f64) -> String {
    let s = format!("{n}");
    if let Some(stripped) = s.strip_suffix(".0") {
        stripped.to_string()
    } else {
        s
    }
}

/// The Crockford base32 alphabet used by ULIDs: no `I`, `L`, `O` or `U`.
const ULID_ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// `entity`: a reference to another entity, by ULID.
pub(super) fn normalize_entity_ref(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim().to_ascii_uppercase();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Entity, ENTITY_FIX));
    }
    if trimmed.len() != 26 || !trimmed.chars().all(|c| ULID_ALPHABET.contains(c)) {
        return Err(ValidationError::new(
            PropertyType::Entity,
            ErrorCode::Malformed,
            format!("`{}` is not a ULID", raw.trim()),
            ENTITY_FIX,
        ));
    }
    // The first character encodes the top bits of the timestamp; anything
    // above `7` would overflow 48 bits.
    if !matches!(trimmed.as_bytes()[0], b'0'..=b'7') {
        return Err(ValidationError::new(
            PropertyType::Entity,
            ErrorCode::OutOfRange,
            format!("`{}` overflows the ULID timestamp range", raw.trim()),
            ENTITY_FIX,
        ));
    }
    Ok(Normalized::new(PropertyType::Entity, raw, trimmed))
}
