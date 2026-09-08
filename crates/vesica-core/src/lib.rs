//! `vesica-core`: the pure domain layer of Vesica.
//!
//! This crate owns the schema registry, property types and value
//! normalisation, caption derivation, ID generation and validation errors
//! that every other crate builds on. It performs no I/O — no database, no
//! filesystem, no network — so the domain rules stay testable without a
//! running server, and so the matcher and the write path can never disagree
//! about what a normalised value looks like. See `03-architecture.md`
//! section 3.4 for the full crate charter.
//!
//! # The value layer
//!
//! Three ideas run through this crate and are worth stating once.
//!
//! **The raw value is sacred; the normalised value is derived and
//! disposable.** [`Normalized`] always carries the caller's input back
//! unchanged in [`Normalized::raw`]. Normalisation rules are expected to
//! change, and when they do the normalised values are recomputed from the raw
//! ones. Nothing here edits, trims or "cleans" what a person actually typed.
//!
//! **A failed checksum is a warning, not an error.** An investigator copying
//! a tax number off a scanned filing has to be able to record what the filing
//! says, including when the filing is wrong. Refusing the value pushes it into
//! a notebook the system cannot see. So [`PropertyType::normalize`] returns the
//! normalised value *plus* zero or more [`Warning`]s, and a bad IBAN, NIP,
//! PESEL, REGON or VIN check digit produces [`WarningCode::ChecksumFailed`]
//! while the value is stored and stays fully usable. Errors are reserved for
//! input the type genuinely cannot represent at all.
//!
//! **A partial date is never widened to a point.** A [`PartialDate`] carries a
//! lower bound, an upper bound and an explicit [`DatePrecision`]. "Sometime in
//! 2019" stays a year, and there is deliberately no function anywhere that
//! turns it into 1 January 2019.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod checksum_algo;
mod date;
mod fold;
mod property;
mod warning;

pub use crate::checksum_algo::{
    bech32_is_valid, iban_mod97_is_valid, nip_is_valid, pesel_is_valid, regon_is_valid,
    vin_is_valid,
};
pub use crate::date::{DateError, DateOrder, DatePrecision, DateStyle, ParsedDate, PartialDate};
pub use crate::property::{
    BlockingKey, BlockingKeyKind, ErrorCode, IdentifierScheme, MatchWeight, NormalizeContext,
    Normalized, PropertyType, PropertyTypeParseError, ValidationError,
};
pub use crate::warning::{Warning, WarningCode};

/// Folding helpers used to build comparison keys.
///
/// These are public because the resolver in a later milestone compares names
/// that were folded here, and it has to fold candidate input in exactly the
/// same way or the comparison means nothing.
pub mod folding {
    pub use crate::fold::{collapse_whitespace, fold_for_comparison, phonetic_key, transliterate};
}
