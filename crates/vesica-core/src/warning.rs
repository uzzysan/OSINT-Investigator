//! Non-fatal notes attached to a normalised value.
//!
//! A warning says "we stored this, and here is what looks off about it". It
//! never prevents a value from being recorded. See the crate docs for why a
//! failed checksum lands here rather than in [`crate::ValidationError`].

use serde::{Deserialize, Serialize};

/// A stable, machine-readable reason a normalised value was flagged.
///
/// Codes are part of the API surface an agent reads, so they are stable
/// strings rather than positional numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WarningCode {
    /// A scheme with a defined check digit did not verify: IBAN mod-97, VIN,
    /// NIP, PESEL, REGON or a bech32 wallet address. The value is stored.
    ChecksumFailed,
    /// No scheme label was supplied, so the scheme was guessed from the shape
    /// of the value. Accompanies [`WarningCode::ChecksumFailed`] to say that
    /// the failure may simply mean the guess was wrong.
    SchemeInferred,
    /// A digest's hex length does not match the algorithm it claims.
    DigestLengthMismatch,
    /// A digest arrived without an algorithm prefix and one was inferred from
    /// its length.
    DigestAlgorithmInferred,
    /// A date-time carried no timezone offset and was read as UTC.
    TimezoneAssumed,
    /// A slash- or dot-separated date could be read either day-first or
    /// month-first; the case's region decided it. The message says which.
    AmbiguousDateOrder,
    /// The number parsed, but is not a valid subscriber number for its
    /// country according to the phone metadata.
    PhoneNotValid,
    /// A URL arrived without a scheme and `https` was assumed.
    UrlSchemeAssumed,
    /// The wallet address matched no address format this build knows about,
    /// so no checksum could be attempted.
    WalletFormatUnknown,
}

impl WarningCode {
    /// The stable string form, as it is written to JSON and to the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChecksumFailed => "checksum_failed",
            Self::SchemeInferred => "scheme_inferred",
            Self::DigestLengthMismatch => "digest_length_mismatch",
            Self::DigestAlgorithmInferred => "digest_algorithm_inferred",
            Self::TimezoneAssumed => "timezone_assumed",
            Self::AmbiguousDateOrder => "ambiguous_date_order",
            Self::PhoneNotValid => "phone_not_valid",
            Self::UrlSchemeAssumed => "url_scheme_assumed",
            Self::WalletFormatUnknown => "wallet_format_unknown",
        }
    }
}

impl std::fmt::Display for WarningCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A note attached to a value that was accepted and stored anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// Why the value was flagged.
    pub code: WarningCode,
    /// A human-readable explanation naming the specific thing that looked
    /// wrong, for display next to the value in the UI and in agent output.
    pub message: String,
}

impl Warning {
    /// Build a warning with the given code and message.
    pub fn new(code: WarningCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
