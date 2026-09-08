//! `phone`: E.164, with the Case's region as the default.
//!
//! `0601 234 567` in a Polish Case becomes `+48601234567`. The same digits in
//! a German Case become a German number, which is the point of the region
//! being a property of the Case rather than of the installation. A number that
//! already carries a `+` country code is unaffected by the region — that is
//! what "international" means, and it is asserted in the tests.

use std::str::FromStr;

use phonenumber::{Mode, country};

use crate::property::{
    AUX_COUNTRY, BlockingKey, BlockingKeyKind, ErrorCode, Normalized, PropertyType, ValidationError,
};
use crate::warning::{Warning, WarningCode};

const FIX: &str = "supply an international number such as `+48 601 234 567`, or set the \
                   Case's default region so a national number like `601 234 567` can be \
                   resolved";

pub(super) fn normalize(
    raw: &str,
    ctx: &super::NormalizeContext,
) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Phone, FIX));
    }

    let region = ctx.region().and_then(|r| country::Id::from_str(r).ok());
    let mut parsed = phonenumber::parse(region, trimmed).map_err(|e| {
        let code = if region.is_none() && !trimmed.starts_with('+') {
            ErrorCode::MissingContext
        } else {
            ErrorCode::Malformed
        };
        ValidationError::new(
            PropertyType::Phone,
            code,
            format!("`{trimmed}` is not a phone number that can be resolved: {e}"),
            FIX,
        )
    })?;

    // Trunk prefix. Poland abolished the leading `0` in 2009 and the phone
    // metadata therefore declares no national prefix for PL, so `0601 234 567`
    // parses as the eleven-digit number `+480601234567` rather than as the
    // subscriber number an analyst copied off a decade-old document. Where the
    // number as given is not valid and dropping leading zeros makes it valid,
    // the zeros were a trunk prefix. The test is deliberately one-way: a
    // number that already parses as valid is never touched.
    if region.is_some() && !trimmed.starts_with('+') && !phonenumber::is_valid(&parsed) {
        let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
        let without_trunk = digits.trim_start_matches('0');
        if without_trunk.len() < digits.len()
            && !without_trunk.is_empty()
            && let Ok(retry) = phonenumber::parse(region, without_trunk)
            && phonenumber::is_valid(&retry)
        {
            parsed = retry;
        }
    }

    let e164 = parsed.format().mode(Mode::E164).to_string();
    let country_code = parsed.country().id().map(|id| id.as_ref().to_string());

    let mut out = Normalized::new(PropertyType::Phone, raw, e164)
        .with_aux(AUX_COUNTRY, country_code.unwrap_or_default());
    if !phonenumber::is_valid(&parsed) {
        // Parseable but not a real subscriber number for its country. Stored,
        // flagged: number ranges change, and metadata goes stale.
        out = out.with_warning(Warning::new(
            WarningCode::PhoneNotValid,
            format!(
                "`{trimmed}` parses but is not a valid number for its country \
                 according to the phone metadata"
            ),
        ));
    }
    Ok(out)
}

pub(super) fn blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    vec![BlockingKey::new(BlockingKeyKind::Phone, n.value.clone())]
}
