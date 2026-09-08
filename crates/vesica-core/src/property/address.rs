//! `address`: token folding, with the postcode and house number pulled out.
//!
//! A postal address is prose with two reliable landmarks in it. The street
//! name is spelled a dozen ways (`ul.`, `ulica`, `Ul`), the city is often
//! omitted, but the postcode and the house number are copied faithfully
//! because they are what the post depends on. So those two become their own
//! keys, and the rest is folded into a bag of tokens.
//!
//! Extraction happens on the raw string, before folding, because folding turns
//! `00-549` into `00 549` and the hyphen is what makes it recognisable as a
//! Polish postcode.

use crate::fold::{fold_for_comparison, sorted_tokens, transliterate};
use crate::property::{
    AUX_HOUSE, AUX_POSTCODE, AUX_TRANSLIT, BlockingKey, BlockingKeyKind, ErrorCode, Normalized,
    PropertyType, ValidationError,
};

const FIX: &str = "supply a postal address, e.g. `ul. Piękna 12/4, 00-549 Warszawa`";

pub(super) fn normalize(raw: &str) -> Result<Normalized, ValidationError> {
    let folded = fold_for_comparison(raw);
    if folded.is_empty() {
        return Err(if raw.trim().is_empty() {
            ValidationError::empty(PropertyType::Address, FIX)
        } else {
            ValidationError::new(
                PropertyType::Address,
                ErrorCode::Malformed,
                "the address contains no letters or digits",
                FIX,
            )
        });
    }

    let tokens: Vec<&str> = raw
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();

    let postcode = tokens.iter().find_map(|t| as_postcode(t));
    // A postcode is digits too, so it has to be excluded before looking for a
    // house number or `00-549` would be read as house 0 of building 549.
    let house = tokens
        .iter()
        .filter(|t| as_postcode(t).is_none())
        .find_map(|t| as_house_number(t));

    // Transliterated from the folded value rather than from the raw input, so
    // that re-normalising a stored value reproduces the same key — see the
    // note in `property::name`.
    let translit = transliterate(&folded);
    Ok(Normalized::new(PropertyType::Address, raw, folded)
        .with_aux(AUX_TRANSLIT, translit)
        .with_aux(AUX_POSTCODE, postcode.unwrap_or_default())
        .with_aux(AUX_HOUSE, house.unwrap_or_default()))
}

/// Recognise the postcode formats this build knows: Polish `NN-NNN` and US
/// `NNNNN` / `NNNNN-NNNN`. Anything else is left in the token bag rather than
/// guessed at.
fn as_postcode(token: &str) -> Option<String> {
    let t = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
    let bytes = t.as_bytes();
    if !bytes.iter().all(|b| b.is_ascii_digit() || *b == b'-') {
        return None;
    }
    let dashes = bytes.iter().filter(|b| **b == b'-').count();
    match (bytes.len(), dashes) {
        // Polish `00-549`.
        (6, 1) if bytes[2] == b'-' => Some(t.to_string()),
        // US ZIP and ZIP+4.
        (5, 0) => Some(t.to_string()),
        (10, 1) if bytes[5] == b'-' => Some(t.to_string()),
        _ => None,
    }
}

/// A house number: digits, optionally a letter, optionally `/` or `-` and
/// another such group — `12`, `12A`, `12/4`, `12-14`.
fn as_house_number(token: &str) -> Option<String> {
    let t = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let ok = t.split(['/', '-']).all(is_number_with_optional_letter);
    if ok && t.len() <= 12 {
        Some(t.to_ascii_uppercase())
    } else {
        None
    }
}

fn is_number_with_optional_letter(part: &str) -> bool {
    let mut chars = part.chars();
    let mut saw_digit = false;
    for c in chars.by_ref() {
        if c.is_ascii_digit() {
            saw_digit = true;
        } else {
            return saw_digit && c.is_ascii_alphabetic() && chars.next().is_none();
        }
    }
    saw_digit
}

pub(super) fn blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    let mut keys = Vec::new();
    let postcode = n.aux(AUX_POSTCODE);
    let house = n.aux(AUX_HOUSE);

    if let Some(postcode) = postcode {
        keys.push(BlockingKey::new(BlockingKeyKind::AddrKey, postcode));
        if let Some(house) = house {
            keys.push(BlockingKey::new(
                BlockingKeyKind::AddrKey,
                format!("{postcode}/{house}"),
            ));
        }
    }
    // The whole address as an order-insensitive bag of transliterated tokens,
    // so that `12 Piękna, Warszawa` and `Warszawa, ul. Pieknej 12` still meet.
    if let Some(translit) = n.aux(AUX_TRANSLIT) {
        keys.push(BlockingKey::new(
            BlockingKeyKind::AddrKey,
            sorted_tokens(translit),
        ));
    }
    keys
}
