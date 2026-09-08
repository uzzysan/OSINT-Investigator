//! Property types: what a value is, how it is validated, how it is normalised
//! and what blocking keys it contributes.
//!
//! The type table is `04-data-model.md` section 4.1.1. Each type owns three
//! pure functions, reached through [`PropertyType`]:
//!
//! 1. [`PropertyType::validate`] — parse and validate, errors only.
//! 2. [`PropertyType::normalize`] — the normalised value plus warnings.
//! 3. [`PropertyType::blocking_keys`] — candidate-lookup keys for the
//!    resolver.
//!
//! Nothing consumes the blocking keys yet. They are defined here because this
//! is where the knowledge of what makes two values comparable lives, and the
//! resolver in M5 should find them waiting rather than invent them.

mod address;
mod email;
mod identifier;
mod misc;
mod name;
mod phone;
mod weburl;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::{DateStyle, PartialDate};
use crate::warning::Warning;

pub use identifier::IdentifierScheme;

/// How much a match on this property type is worth to the resolver.
///
/// Transcribed from the "Match weight" column of `04-data-model.md` 4.1.1.
/// A `VeryStrong` property alone can carry a pair over the suggest threshold;
/// a name alone never can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchWeight {
    /// Contributes nothing to a match score (free prose).
    None,
    /// Weak corroboration only.
    Weak,
    /// Meaningful, but not on its own.
    Medium,
    /// Strong evidence of the same entity.
    Strong,
    /// Sufficient on its own to propose a match.
    VeryStrong,
}

/// The type of a property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PropertyType {
    /// A person's or organisation's name.
    Name,
    /// An email address.
    Email,
    /// A telephone number.
    Phone,
    /// A web address.
    Url,
    /// A DNS domain name.
    Domain,
    /// An IPv4 or IPv6 address.
    Ip,
    /// A national or scheme identifier: tax number, passport number, VIN.
    Identifier,
    /// A date, to whatever precision was known.
    Date,
    /// A country.
    Country,
    /// A postal address.
    Address,
    /// Free prose.
    Text,
    /// A number.
    Number,
    /// A reference to another entity, by id.
    Entity,
    /// A cryptographic digest, `sha256:…`.
    Checksum,
    /// A bank account in IBAN form.
    Iban,
    /// A cryptocurrency address.
    Wallet,
}

/// The string form of a [`PropertyType`] was not recognised.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{found}` is not a known property type")]
pub struct PropertyTypeParseError {
    /// The unrecognised input.
    pub found: String,
}

impl PropertyType {
    /// Every property type, in the order of the data-model table.
    pub const ALL: [PropertyType; 16] = [
        Self::Name,
        Self::Email,
        Self::Phone,
        Self::Url,
        Self::Domain,
        Self::Ip,
        Self::Identifier,
        Self::Date,
        Self::Country,
        Self::Address,
        Self::Text,
        Self::Number,
        Self::Entity,
        Self::Checksum,
        Self::Iban,
        Self::Wallet,
    ];

    /// The stable string form, as stored in `entity_prop.ptype` and used in
    /// schema YAML.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Email => "email",
            Self::Phone => "phone",
            Self::Url => "url",
            Self::Domain => "domain",
            Self::Ip => "ip",
            Self::Identifier => "identifier",
            Self::Date => "date",
            Self::Country => "country",
            Self::Address => "address",
            Self::Text => "text",
            Self::Number => "number",
            Self::Entity => "entity",
            Self::Checksum => "checksum",
            Self::Iban => "iban",
            Self::Wallet => "wallet",
        }
    }

    /// How much a match on this type is worth. See [`MatchWeight`].
    pub fn match_weight(self) -> MatchWeight {
        match self {
            Self::Text | Self::Entity => MatchWeight::None,
            Self::Country | Self::Number => MatchWeight::Weak,
            Self::Name | Self::Url | Self::Date | Self::Address => MatchWeight::Medium,
            Self::Ip => MatchWeight::Strong,
            Self::Email | Self::Phone | Self::Domain => MatchWeight::Strong,
            Self::Identifier | Self::Checksum | Self::Iban | Self::Wallet => {
                MatchWeight::VeryStrong
            }
        }
    }

    /// Parse and validate, discarding the normalised form.
    ///
    /// This is the "does this value belong to this type at all" question. It
    /// says nothing about check digits: a NIP with a transposed digit is a
    /// valid NIP-shaped value and passes here, carrying a warning out of
    /// [`PropertyType::normalize`].
    pub fn validate(self, raw: &str, ctx: &NormalizeContext) -> Result<(), ValidationError> {
        self.normalize(raw, ctx).map(|_| ())
    }

    /// Normalise a raw value.
    ///
    /// Returns the raw value unchanged alongside the normalised one, plus any
    /// [`Warning`]s. Errors are reserved for values this type cannot represent
    /// at all; anything merely suspicious comes back as a warning on an
    /// otherwise usable result.
    pub fn normalize(
        self,
        raw: &str,
        ctx: &NormalizeContext,
    ) -> Result<Normalized, ValidationError> {
        match self {
            Self::Name => name::normalize(raw),
            Self::Email => email::normalize(raw),
            Self::Phone => phone::normalize(raw, ctx),
            Self::Url => weburl::normalize_url(raw),
            Self::Domain => weburl::normalize_domain(raw),
            Self::Ip => misc::normalize_ip(raw),
            Self::Identifier => identifier::normalize_identifier(raw),
            Self::Date => misc::normalize_date(raw, ctx),
            Self::Country => misc::normalize_country(raw),
            Self::Address => address::normalize(raw),
            Self::Text => misc::normalize_text(raw),
            Self::Number => misc::normalize_number(raw),
            Self::Entity => misc::normalize_entity_ref(raw),
            Self::Checksum => identifier::normalize_checksum(raw),
            Self::Iban => identifier::normalize_iban(raw),
            Self::Wallet => identifier::normalize_wallet(raw),
        }
    }

    /// Normalise, then derive the blocking keys.
    ///
    /// A convenience over [`Normalized::blocking_keys`] for callers that have
    /// only the raw value.
    pub fn blocking_keys(
        self,
        raw: &str,
        ctx: &NormalizeContext,
    ) -> Result<Vec<BlockingKey>, ValidationError> {
        Ok(self.normalize(raw, ctx)?.blocking_keys())
    }
}

impl std::fmt::Display for PropertyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for PropertyType {
    type Err = PropertyTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|t| t.as_str() == s)
            .ok_or_else(|| PropertyTypeParseError {
                found: s.to_string(),
            })
    }
}

/// Everything normalisation needs to know about where a value came from.
///
/// Today that is the Case's `default_region`, which decides how a national
/// phone number becomes E.164 and how `12/04/1978` is read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizeContext {
    region: Option<String>,
}

impl NormalizeContext {
    /// A context with no region. National-format phone numbers cannot be
    /// normalised without one and will report that as an error.
    pub fn new() -> Self {
        Self::default()
    }

    /// A context for a Case whose `default_region` is the given ISO-3166-1
    /// alpha-2 code. The code is upper-cased; an empty string means "none".
    pub fn with_region(region: &str) -> Self {
        let r = region.trim();
        Self {
            region: if r.is_empty() {
                None
            } else {
                Some(r.to_ascii_uppercase())
            },
        }
    }

    /// The Case's default region, if it has one.
    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// How to read an ambiguous `dd/mm/yyyy` date in this Case.
    pub fn date_style(&self) -> DateStyle {
        DateStyle::for_region(self.region())
    }
}

/// Why a value could not be represented as this property type at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorCode {
    /// The value was empty or whitespace only.
    Empty,
    /// The value does not have the shape this type requires.
    Malformed,
    /// The value could be read more than one way and guessing is not
    /// acceptable — a two-digit year, for instance.
    Ambiguous,
    /// A component was outside its permitted range.
    OutOfRange,
    /// The value is well formed but names something no lookup table knows.
    UnknownValue,
    /// Normalisation needed context the caller did not supply, such as a Case
    /// region for a national phone number.
    MissingContext,
}

/// A value that does not belong to its property type.
///
/// Carries a `fix` string saying what a valid value looks like, so an agent
/// reading the error can correct itself without a human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{ptype}: {message}")]
pub struct ValidationError {
    /// The type the value was being read as.
    pub ptype: PropertyType,
    /// A stable machine-readable reason.
    pub code: ErrorCode,
    /// What was wrong, naming the offending part of the input.
    pub message: String,
    /// What a valid value looks like.
    pub fix: String,
}

impl ValidationError {
    pub(crate) fn new(
        ptype: PropertyType,
        code: ErrorCode,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            ptype,
            code,
            message: message.into(),
            fix: fix.into(),
        }
    }

    pub(crate) fn empty(ptype: PropertyType, fix: impl Into<String>) -> Self {
        Self::new(
            ptype,
            ErrorCode::Empty,
            "the value was empty or whitespace only",
            fix,
        )
    }
}

/// The kinds of blocking key, matching the `kind` column of the `block_key`
/// table in `04-data-model.md` section 4.2.4.
///
/// A blocking key is not a match. It is a cheap bucket that says "these two
/// entities are worth comparing properly", so that the resolver never has to
/// consider every pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockingKeyKind {
    /// A scheme-qualified strong identifier: `nip:5252248481`, `iban:PL61…`,
    /// `sha256:…`, `ip:2001:db8::1`.
    Ident,
    /// An E.164 phone number.
    Phone,
    /// A folded email address.
    Email,
    /// A domain, both as given and as its registrable form.
    Domain,
    /// A name's tokens, transliterated and sorted.
    NameSorted,
    /// A name's Double Metaphone codes, sorted.
    NamePhonetic,
    /// An ISO prefix of a date, one per precision level.
    Dob,
    /// A postcode, a postcode plus house number, or the sorted tokens of an
    /// address.
    AddrKey,
}

impl BlockingKeyKind {
    /// The stable string form, as stored in `block_key.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ident => "ident",
            Self::Phone => "phone",
            Self::Email => "email",
            Self::Domain => "domain",
            Self::NameSorted => "name_sorted",
            Self::NamePhonetic => "name_phonetic",
            Self::Dob => "dob",
            Self::AddrKey => "addr_key",
        }
    }
}

impl std::fmt::Display for BlockingKeyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of the `block_key` table.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlockingKey {
    /// Which index this key belongs in.
    pub kind: BlockingKeyKind,
    /// The key itself.
    pub key: String,
}

impl BlockingKey {
    pub(crate) fn new(kind: BlockingKeyKind, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
        }
    }
}

/// Auxiliary key: the registrable form of a domain or a URL host.
pub(crate) const AUX_REGISTRABLE: &str = "registrable";
/// Auxiliary key: the identifier scheme that was recognised.
pub(crate) const AUX_SCHEME: &str = "scheme";
/// Auxiliary key: the `+tag` stripped from an email local part.
pub(crate) const AUX_TAG: &str = "tag";
/// Auxiliary key: the domain part of an email address or URL.
pub(crate) const AUX_DOMAIN: &str = "domain";
/// Auxiliary key: a postcode extracted from an address.
pub(crate) const AUX_POSTCODE: &str = "postcode";
/// Auxiliary key: a house number extracted from an address.
pub(crate) const AUX_HOUSE: &str = "house";
/// Auxiliary key: the ASCII-transliterated form of a name or address.
pub(crate) const AUX_TRANSLIT: &str = "translit";
/// Auxiliary key: the phonetic encoding of a name.
pub(crate) const AUX_PHONETIC: &str = "phonetic";
/// Auxiliary key: the country a phone number belongs to.
pub(crate) const AUX_COUNTRY: &str = "country";
/// Auxiliary key: the digest algorithm of a checksum value.
pub(crate) const AUX_ALGORITHM: &str = "algorithm";

/// A normalised value: what was typed, what it normalises to, and everything
/// derived from it.
///
/// `raw` is returned byte-for-byte as it was given. Normalisation rules change
/// over time and `vesica rescan-matches` recomputes everything else from
/// `raw`, so nothing here is allowed to be the only copy of anything.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Normalized {
    /// The type this value was normalised as.
    pub ptype: PropertyType,
    /// The caller's input, unchanged. Goes in `entity_prop.value`.
    pub raw: String,
    /// The normalised form, for exact matching. Goes in
    /// `entity_prop.value_norm`.
    pub value: String,
    /// The numeric value, for `entity_prop.value_num`. Set only for
    /// [`PropertyType::Number`].
    pub numeric: Option<f64>,
    /// The parsed date, for `entity_prop.date_start` / `date_end` /
    /// `date_prec`. Set only for [`PropertyType::Date`].
    pub date: Option<PartialDate>,
    /// Type-specific extras: a domain's registrable form, an identifier's
    /// scheme, an address's postcode. Keys are the `AUX_*` constants.
    pub aux: BTreeMap<String, String>,
    /// Anything that looked wrong about a value that was stored anyway.
    pub warnings: Vec<Warning>,
}

impl Normalized {
    pub(crate) fn new(ptype: PropertyType, raw: &str, value: impl Into<String>) -> Self {
        Self {
            ptype,
            raw: raw.to_string(),
            value: value.into(),
            numeric: None,
            date: None,
            aux: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn with_aux(mut self, key: &str, value: impl Into<String>) -> Self {
        let v = value.into();
        if !v.is_empty() {
            self.aux.insert(key.to_string(), v);
        }
        self
    }

    pub(crate) fn with_warning(mut self, warning: Warning) -> Self {
        self.warnings.push(warning);
        self
    }

    pub(crate) fn with_warnings(mut self, warnings: impl IntoIterator<Item = Warning>) -> Self {
        self.warnings.extend(warnings);
        self
    }

    /// Look up an auxiliary value by one of the `AUX_*` keys.
    pub fn aux(&self, key: &str) -> Option<&str> {
        self.aux.get(key).map(String::as_str)
    }

    /// True when the value was stored with something flagged against it.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// The blocking keys this value contributes to the candidate index.
    ///
    /// Derived from the normalised form, so two values that normalise the same
    /// necessarily block together. May legitimately be empty — free prose and
    /// entity references contribute nothing.
    pub fn blocking_keys(&self) -> Vec<BlockingKey> {
        let mut keys = match self.ptype {
            PropertyType::Name => name::blocking_keys(self),
            PropertyType::Email => email::blocking_keys(self),
            PropertyType::Phone => phone::blocking_keys(self),
            PropertyType::Url | PropertyType::Domain => weburl::blocking_keys(self),
            PropertyType::Address => address::blocking_keys(self),
            PropertyType::Date => misc::date_blocking_keys(self),
            PropertyType::Ip => vec![BlockingKey::new(
                BlockingKeyKind::Ident,
                format!("ip:{}", self.value),
            )],
            PropertyType::Identifier
            | PropertyType::Checksum
            | PropertyType::Iban
            | PropertyType::Wallet => identifier::blocking_keys(self),
            PropertyType::Country
            | PropertyType::Text
            | PropertyType::Number
            | PropertyType::Entity => Vec::new(),
        };
        keys.retain(|k| !k.key.is_empty());
        keys.sort();
        keys.dedup();
        keys
    }
}
