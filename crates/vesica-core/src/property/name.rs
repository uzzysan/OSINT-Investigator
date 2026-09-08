//! `name`: people and organisations.
//!
//! The stored value keeps its diacritics. `Łukasz Żółć` is written down as
//! `Łukasz Żółć` and displayed that way forever; only the comparison forms
//! lose them. Polish is a first-class case here rather than an edge case,
//! which is why the transliterated form exists at all: NFKD decomposes `ó`
//! and `ż`, but `ł` has no decomposition and would otherwise never match an
//! ASCII transcription.

use crate::fold::{fold_for_comparison, phonetic_key, sorted_tokens, transliterate};
use crate::property::{
    AUX_PHONETIC, AUX_TRANSLIT, BlockingKey, BlockingKeyKind, ErrorCode, Normalized, PropertyType,
    ValidationError,
};

const FIX: &str = "supply a non-empty name containing at least one letter or digit, \
                   e.g. `Jan Kowalski`";

pub(super) fn normalize(raw: &str) -> Result<Normalized, ValidationError> {
    let folded = fold_for_comparison(raw);
    if folded.is_empty() {
        return Err(if raw.trim().is_empty() {
            ValidationError::empty(PropertyType::Name, FIX)
        } else {
            ValidationError::new(
                PropertyType::Name,
                ErrorCode::Malformed,
                "the name contains no letters or digits",
                FIX,
            )
        });
    }
    // Both comparison forms are derived from the folded value, not from the
    // raw input. Deriving them from the raw string would make them unstable
    // under re-normalisation: `deunicode` transliterates a bare combining mark
    // to a letter, so `fold(raw)` and `translit(raw)` could disagree about how
    // many letters the name has. Everything downstream is built on the folded
    // value, so the transliteration is of that.
    let translit = transliterate(&folded);
    let phonetic = phonetic_key(&folded);
    Ok(Normalized::new(PropertyType::Name, raw, folded)
        .with_aux(AUX_TRANSLIT, translit)
        .with_aux(AUX_PHONETIC, phonetic))
}

pub(super) fn blocking_keys(n: &Normalized) -> Vec<BlockingKey> {
    let mut keys = Vec::new();
    // Sorted so that `Kowalski, Jan` and `Jan Kowalski` land in the same
    // bucket; transliterated so that `Żółć` and `Zolc` do too.
    if let Some(translit) = n.aux(AUX_TRANSLIT) {
        keys.push(BlockingKey::new(
            BlockingKeyKind::NameSorted,
            sorted_tokens(translit),
        ));
    }
    if let Some(phonetic) = n.aux(AUX_PHONETIC) {
        keys.push(BlockingKey::new(BlockingKeyKind::NamePhonetic, phonetic));
    }
    keys
}
