//! Shared folding primitives.
//!
//! Every comparison key in Vesica is built from these three functions, so the
//! write path and the matcher cannot disagree about what "the same string"
//! means.

use deunicode::deunicode;
use rphonetic::{DoubleMetaphone, Encoder};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Collapse runs of any Unicode whitespace into single spaces and trim.
pub fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One pass of decompose-strip-lowercase. Applied twice by
/// [`fold_for_comparison`] because lowercasing can itself introduce combining
/// marks (`İ` lowercases to `i` plus a combining dot above).
fn decompose_strip_lower(s: &str) -> String {
    s.nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// Fold a string down to a comparison form: Unicode NFKD, combining marks
/// dropped, lowercased, every non-alphanumeric character turned into a space,
/// whitespace collapsed.
///
/// Diacritics that decompose (`ó`, `ż`, `é`) disappear here. Letters that do
/// not decompose (`ł`, `ø`, `đ`) survive — that is what [`transliterate`] is
/// for. The stored raw value is never touched by any of this.
pub fn fold_for_comparison(s: &str) -> String {
    let once = decompose_strip_lower(s);
    let twice = decompose_strip_lower(&once);
    let spaced: String = twice
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    collapse_whitespace(&spaced)
}

/// Fold to pure ASCII by transliterating first, so that Polish `Łukasz Żółć`
/// and an ASCII-only transcription `Lukasz Zolc` produce the same key.
pub fn transliterate(s: &str) -> String {
    fold_for_comparison(&deunicode(s))
}

/// A phonetic key for a name, for the `name_phonetic` blocking kind.
///
/// Double Metaphone over the transliterated form: the input is guaranteed
/// ASCII by [`transliterate`], and tokens are sorted so that `Kowalski Jan`
/// and `Jan Kowalski` produce the same key.
pub fn phonetic_key(s: &str) -> String {
    let ascii = transliterate(s);
    if ascii.is_empty() {
        return String::new();
    }
    let encoder = DoubleMetaphone::default();
    let mut codes: Vec<String> = ascii
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(|t| {
            let code = encoder.encode(t);
            if code.is_empty() { t.to_string() } else { code }
        })
        .collect();
    codes.sort();
    codes.dedup();
    codes.join(" ")
}

/// Tokens of a folded string, sorted and de-duplicated, joined by spaces.
///
/// Used for `name_sorted` and address keys, where word order carries no
/// information for matching purposes.
pub(crate) fn sorted_tokens(folded: &str) -> String {
    let mut tokens: Vec<&str> = folded.split(' ').filter(|t| !t.is_empty()).collect();
    tokens.sort_unstable();
    tokens.dedup();
    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_is_idempotent_on_polish() {
        let folded = fold_for_comparison("Łukasz Żółć-Kowalski");
        // `ó` and `ż` decompose and lose their marks; `ł` has no
        // decomposition and survives — which is exactly why `transliterate`
        // exists alongside this.
        assert_eq!(folded, "łukasz zołc kowalski");
        assert_eq!(fold_for_comparison(&folded), folded);
    }

    #[test]
    fn transliteration_reaches_ascii() {
        assert_eq!(transliterate("Łukasz Żółć"), "lukasz zolc");
        assert_eq!(transliterate("Lukasz Zolc"), "lukasz zolc");
    }

    #[test]
    fn phonetic_key_is_order_insensitive() {
        assert_eq!(phonetic_key("Jan Kowalski"), phonetic_key("Kowalski Jan"));
    }
}
