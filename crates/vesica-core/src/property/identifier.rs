//! `identifier`, `iban`, `wallet` and `checksum`.
//!
//! This is where the ticket's central decision lives. A check digit that does
//! not verify produces [`WarningCode::ChecksumFailed`] and the value is stored
//! anyway. An investigator transcribing a tax number from a scanned filing has
//! to be able to record what the filing says; a filing with a typo in it is
//! evidence, and refusing to hold it just moves it into a notebook nobody can
//! search.
//!
//! Where no scheme label was given, the scheme is guessed from the shape of
//! the value — ten digits look like a NIP, eleven like a PESEL. A guess that
//! turns out to be wrong would otherwise produce a confusing warning, so a
//! guessed scheme is reported alongside the failure as
//! [`WarningCode::SchemeInferred`].

use crate::checksum_algo::{
    bech32_is_valid, iban_mod97_is_valid, nip_is_valid, pesel_is_valid, regon_is_valid,
    vin_is_valid,
};
use crate::property::{
    AUX_ALGORITHM, AUX_SCHEME, BlockingKey, BlockingKeyKind, ErrorCode, Normalized, PropertyType,
    ValidationError,
};
use crate::warning::{Warning, WarningCode};

const IDENT_FIX: &str = "supply the identifier's characters; separators and case are \
                         ignored. A scheme label such as `NIP 5252248481` makes the \
                         check digit verifiable.";
const IBAN_FIX: &str = "supply an IBAN, e.g. `PL61 1090 1014 0000 0712 1981 2874`";
const WALLET_FIX: &str = "supply a cryptocurrency address, e.g. a `bc1…` bech32 address \
                          or a `0x…` 40-hex-digit account";
const CHECKSUM_FIX: &str = "supply a digest as `algorithm:hex`, e.g. \
                            `sha256:e3b0c442…` (64 hex digits)";

/// An identifier scheme whose check digit this build knows how to verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum IdentifierScheme {
    /// International Bank Account Number, ISO 13616, mod-97.
    Iban,
    /// Vehicle Identification Number, ISO 3779, 17 characters.
    Vin,
    /// Polish tax identification number, 10 digits.
    Nip,
    /// Polish national identification number, 11 digits.
    Pesel,
    /// Polish business registry number, 9 or 14 digits.
    Regon,
}

impl IdentifierScheme {
    /// The stable lower-case name, used as the prefix of an `ident` blocking
    /// key and stored in `Normalized::aux` under `scheme`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Iban => "iban",
            Self::Vin => "vin",
            Self::Nip => "nip",
            Self::Pesel => "pesel",
            Self::Regon => "regon",
        }
    }

    /// Parse a scheme label as an analyst would type it: `NIP`, `nip`,
    /// `PESEL`.
    pub fn parse_label(label: &str) -> Option<Self> {
        match label.to_ascii_uppercase().as_str() {
            "IBAN" => Some(Self::Iban),
            "VIN" => Some(Self::Vin),
            "NIP" => Some(Self::Nip),
            "PESEL" => Some(Self::Pesel),
            "REGON" => Some(Self::Regon),
            _ => None,
        }
    }

    /// Verify the scheme's check digit over an already-compacted value.
    pub fn is_valid(self, compact: &str) -> bool {
        match self {
            Self::Iban => iban_mod97_is_valid(compact),
            Self::Vin => vin_is_valid(compact),
            Self::Nip => nip_is_valid(compact),
            Self::Pesel => pesel_is_valid(compact),
            Self::Regon => regon_is_valid(compact),
        }
    }

    /// Guess the scheme from the shape of a compacted value.
    ///
    /// Deliberately conservative and deliberately Poland-flavoured: this is a
    /// tool built for Polish investigations first, and a ten-digit number in a
    /// Polish case is far more often a NIP than anything else. A wrong guess
    /// costs a warning, never a rejection.
    pub fn infer(compact: &str) -> Option<Self> {
        let len = compact.len();
        let all_digits = !compact.is_empty() && compact.bytes().all(|b| b.is_ascii_digit());
        if all_digits {
            return match len {
                10 => Some(Self::Nip),
                11 => Some(Self::Pesel),
                9 | 14 => Some(Self::Regon),
                _ => None,
            };
        }
        let bytes = compact.as_bytes();
        if (15..=34).contains(&len)
            && bytes[0].is_ascii_uppercase()
            && bytes[1].is_ascii_uppercase()
            && bytes[2].is_ascii_digit()
            && bytes[3].is_ascii_digit()
            && rust_iso3166::from_alpha2(&compact[..2]).is_some()
        {
            return Some(Self::Iban);
        }
        if len == 17
            && bytes.iter().all(u8::is_ascii_alphanumeric)
            && !compact.contains(['I', 'O', 'Q'])
            && compact.bytes().any(|b| b.is_ascii_digit())
            && compact.bytes().any(|b| b.is_ascii_alphabetic())
        {
            return Some(Self::Vin);
        }
        None
    }
}

impl std::fmt::Display for IdentifierScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// True when a string has the *shape* of a bech32 address — a human-readable
/// prefix, a `1` separator, and a data part drawn from the bech32 alphabet —
/// regardless of whether its checksum verifies. Used so that a corrupted
/// `bc1…` address is recognised and flagged rather than silently filed as
/// "some unknown format".
fn is_bech32_shaped(s: &str) -> bool {
    const CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    if !s.is_ascii() || !(8..=90).contains(&s.len()) {
        return false;
    }
    let lowered = s.to_ascii_lowercase();
    let Some(sep) = lowered.rfind('1') else {
        return false;
    };
    if sep == 0 || lowered.len() - sep - 1 < 6 {
        return false;
    }
    lowered[..sep].bytes().all(|b| b.is_ascii_alphabetic())
        && lowered[sep + 1..].chars().all(|c| CHARSET.contains(c))
}

/// Strip everything that is not a letter or digit and upper-case the rest.
fn compact(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Split a leading scheme label off the front: `NIP 5252248481`, `PESEL:
/// 44051401359`.
fn split_label(raw: &str) -> (Option<IdentifierScheme>, &str) {
    let trimmed = raw.trim();
    let boundary = trimmed.find([' ', ':', '\t', '-', '.']);
    if let Some(idx) = boundary
        && let Some(scheme) = IdentifierScheme::parse_label(&trimmed[..idx])
    {
        return (Some(scheme), &trimmed[idx + 1..]);
    }
    (None, trimmed)
}

/// Build the warning for a failed check digit. Never an error — see the
/// module documentation.
fn checksum_warnings(scheme: IdentifierScheme, value: &str, inferred: bool) -> Vec<Warning> {
    if scheme.is_valid(value) {
        return Vec::new();
    }
    let mut warnings = vec![Warning::new(
        WarningCode::ChecksumFailed,
        format!(
            "`{value}` does not satisfy the {scheme} check digit; the value has been \
             stored as given"
        ),
    )];
    if inferred {
        warnings.push(Warning::new(
            WarningCode::SchemeInferred,
            format!(
                "the scheme `{scheme}` was guessed from the shape of the value; label it \
                 explicitly (e.g. `{} …`) if that guess is wrong",
                scheme.as_str().to_ascii_uppercase()
            ),
        ));
    }
    warnings
}

pub(super) fn normalize_identifier(raw: &str) -> Result<Normalized, ValidationError> {
    let (label, rest) = split_label(raw);
    let value = compact(rest);
    if value.is_empty() {
        return Err(if raw.trim().is_empty() {
            ValidationError::empty(PropertyType::Identifier, IDENT_FIX)
        } else {
            ValidationError::new(
                PropertyType::Identifier,
                ErrorCode::Malformed,
                "the identifier contains no letters or digits",
                IDENT_FIX,
            )
        });
    }

    let inferred = label.is_none();
    let scheme = label.or_else(|| IdentifierScheme::infer(&value));

    let mut out = Normalized::new(PropertyType::Identifier, raw, value.clone());
    if let Some(scheme) = scheme {
        out = out
            .with_aux(AUX_SCHEME, scheme.as_str())
            .with_warnings(checksum_warnings(scheme, &value, inferred));
    }
    Ok(out)
}

pub(super) fn normalize_iban(raw: &str) -> Result<Normalized, ValidationError> {
    let (_, rest) = split_label(raw);
    let value = compact(rest);
    if value.is_empty() {
        return Err(ValidationError::empty(PropertyType::Iban, IBAN_FIX));
    }
    if value.len() < 5 || value.len() > 34 {
        return Err(ValidationError::new(
            PropertyType::Iban,
            ErrorCode::OutOfRange,
            format!(
                "an IBAN is 5 to 34 characters; `{value}` is {}",
                value.len()
            ),
            IBAN_FIX,
        ));
    }

    Ok(Normalized::new(PropertyType::Iban, raw, value.clone())
        .with_aux(AUX_SCHEME, IdentifierScheme::Iban.as_str())
        .with_warnings(checksum_warnings(IdentifierScheme::Iban, &value, false)))
}

pub(super) fn normalize_wallet(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Wallet, WALLET_FIX));
    }
    if !trimmed.is_ascii() {
        return Err(ValidationError::new(
            PropertyType::Wallet,
            ErrorCode::Malformed,
            "a wallet address is ASCII",
            WALLET_FIX,
        ));
    }

    // Ethereum-style: `0x` and 40 hex digits. Case is an EIP-55 checksum this
    // build cannot verify without keccak, so the case is folded away rather
    // than judged.
    let is_evm = trimmed.len() == 42
        && (trimmed.starts_with("0x") || trimmed.starts_with("0X"))
        && trimmed[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if is_evm {
        return Ok(
            Normalized::new(PropertyType::Wallet, raw, trimmed.to_ascii_lowercase())
                .with_aux(AUX_SCHEME, "evm"),
        );
    }

    // Bech32/bech32m: `bc1…`, `tb1…`, `ltc1…`. The polymod is pure arithmetic
    // — no hashing — so this one really can be checked, and a corrupted
    // address is flagged rather than refused.
    if is_bech32_shaped(&trimmed) {
        let lowered = trimmed.to_ascii_lowercase();
        let out = Normalized::new(PropertyType::Wallet, raw, lowered.clone())
            .with_aux(AUX_SCHEME, "bech32");
        return Ok(if bech32_is_valid(&trimmed) {
            out
        } else {
            out.with_warning(Warning::new(
                WarningCode::ChecksumFailed,
                format!(
                    "`{lowered}` does not satisfy the bech32 checksum; the value has \
                     been stored as given"
                ),
            ))
        });
    }

    // Base58Check (a legacy `1…`/`3…` Bitcoin address) has a checksum, but
    // verifying it needs SHA-256, which is not worth a hashing dependency in
    // the value layer. The alphabet is checked; the checksum is not, and the
    // case is preserved because base58 is case-sensitive.
    const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let is_base58 =
        (26..=35).contains(&trimmed.len()) && trimmed.chars().all(|c| BASE58.contains(c));
    if is_base58 {
        return Ok(
            Normalized::new(PropertyType::Wallet, raw, trimmed).with_aux(AUX_SCHEME, "base58")
        );
    }

    // Something else, or a bech32 address whose checksum failed. Either way it
    // is stored; the warning says only that nothing could be verified.
    Ok(
        Normalized::new(PropertyType::Wallet, raw, trimmed.clone()).with_warning(Warning::new(
            WarningCode::WalletFormatUnknown,
            format!("`{trimmed}` matches no address format this build can verify"),
        )),
    )
}

/// Hex lengths of the digest algorithms worth naming.
const DIGESTS: &[(&str, usize)] = &[
    ("md5", 32),
    ("sha1", 40),
    ("sha256", 64),
    ("sha384", 96),
    ("sha512", 128),
    ("blake3", 64),
];

pub(super) fn normalize_checksum(raw: &str) -> Result<Normalized, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::empty(PropertyType::Checksum, CHECKSUM_FIX));
    }

    let (algo, hex, algo_given) = match trimmed.split_once(':') {
        Some((a, h)) => (a.trim().to_ascii_lowercase(), h.trim(), true),
        None => (String::new(), trimmed, false),
    };
    let hex = hex.to_ascii_lowercase();

    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ValidationError::new(
            PropertyType::Checksum,
            ErrorCode::Malformed,
            format!("`{hex}` is not a hexadecimal digest"),
            CHECKSUM_FIX,
        ));
    }

    let mut warnings = Vec::new();
    let algo = if algo_given {
        if let Some((_, expected)) = DIGESTS.iter().find(|(name, _)| *name == algo)
            && *expected != hex.len()
        {
            warnings.push(Warning::new(
                WarningCode::DigestLengthMismatch,
                format!(
                    "a {algo} digest is {expected} hex digits; this one is {}",
                    hex.len()
                ),
            ));
        }
        algo
    } else {
        // No prefix. Infer from the length when it is unambiguous, so that a
        // bare digest still lands in the same blocking bucket as a labelled
        // one.
        match DIGESTS.iter().find(|(_, len)| *len == hex.len()) {
            Some((name, _)) => {
                warnings.push(Warning::new(
                    WarningCode::DigestAlgorithmInferred,
                    format!(
                        "no algorithm prefix; {} hex digits read as {name}",
                        hex.len()
                    ),
                ));
                (*name).to_string()
            }
            None => String::new(),
        }
    };

    let value = if algo.is_empty() {
        hex.clone()
    } else {
        format!("{algo}:{hex}")
    };
    Ok(Normalized::new(PropertyType::Checksum, raw, value)
        .with_aux(AUX_ALGORITHM, algo)
        .with_warnings(warnings))
}

pub(super) fn blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    let prefix = match n.ptype {
        PropertyType::Checksum => "digest",
        PropertyType::Iban => IdentifierScheme::Iban.as_str(),
        PropertyType::Wallet => "wallet",
        _ => n.aux(AUX_SCHEME).unwrap_or("id"),
    };
    vec![BlockingKey::new(
        BlockingKeyKind::Ident,
        format!("{prefix}:{}", n.value),
    )]
}
