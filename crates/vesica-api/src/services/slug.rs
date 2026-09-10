//! Slugs: a Case's stable, human-readable handle.
//!
//! Two rules, both load-bearing.
//!
//! **Derived from the name, once.** A slug is computed when the Case is
//! created and never recomputed. Renaming a Case does not change its slug,
//! because a slug that moves breaks every URL, every report that quoted it
//! and every agent that remembered it — and the whole reason the Case has
//! one is to be quotable (user story 16: "so that it can be referenced
//! stably in URLs and by agents").
//!
//! **Unique across every Case, including deleted ones.** `kase.slug` is
//! `UNIQUE` over the whole table, so a soft-deleted Case still holds its
//! slug — which is exactly right, since restoring it must give back the
//! reference people already have. Purging releases it.

use vesica_store::SqliteConnection;
use vesica_store::repo::case;

use super::error::ServiceError;

/// Longest slug this generates. Long enough to stay readable for a real
/// case name, short enough to sit in a URL and a terminal line.
const MAX_SLUG_LEN: usize = 64;

/// What a Case is called when its name transliterates to nothing at all —
/// a name made entirely of punctuation or of symbols with no ASCII
/// equivalent. Such a Case still needs a referenceable handle, and
/// `case-2`, `case-3` … is a better answer than refusing the name.
const FALLBACK_SLUG: &str = "case";

/// How many suffixed candidates to try before giving up. A name colliding
/// a thousand times is a caller doing something odd, not a case load.
const MAX_SUFFIX: u32 = 1_000;

/// Derives the slug a name would like to have, before uniqueness is
/// considered.
///
/// Folding is [`vesica_core::folding::transliterate`], the same function the
/// matcher uses to compare names, so `Operacja Żółć` and an ASCII
/// transcription of it produce the same base slug rather than two that only
/// a human can see are the same thing.
pub(crate) fn slugify(name: &str) -> String {
    let folded = vesica_core::folding::transliterate(name);
    let joined = folded.split_whitespace().collect::<Vec<_>>().join("-");
    let truncated = truncate_on_boundary(&joined, MAX_SLUG_LEN);
    if truncated.is_empty() {
        FALLBACK_SLUG.to_string()
    } else {
        truncated
    }
}

/// Cuts to at most `max` characters, preferring to cut at a `-` so the slug
/// does not end mid-word, and never leaving a trailing `-`.
fn truncate_on_boundary(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.trim_matches('-').to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let at_boundary = match cut.rfind('-') {
        // Only honour the word boundary if it leaves something worth
        // having; otherwise a long first word would truncate to nothing.
        Some(i) if i >= max / 2 => &cut[..i],
        _ => cut.as_str(),
    };
    at_boundary.trim_matches('-').to_string()
}

/// Finds the first free slug at or after `base`, inside the caller's
/// transaction so that no other write can take it between the check and the
/// insert.
///
/// Collisions get a numeric suffix — `operation-lighthouse`,
/// `operation-lighthouse-2`, … — rather than a random one, because the
/// second case an analyst names "Operation Lighthouse" should get a handle
/// they can still read and guess.
pub(crate) async fn allocate(
    conn: &mut SqliteConnection,
    base: &str,
) -> Result<String, ServiceError> {
    if !case::slug_is_taken(&mut *conn, base).await? {
        return Ok(base.to_string());
    }
    for suffix in 2..=MAX_SUFFIX {
        let candidate = format!("{base}-{suffix}");
        if !case::slug_is_taken(&mut *conn, &candidate).await? {
            return Ok(candidate);
        }
    }
    Err(ServiceError::Conflict {
        message: format!("`{base}` and its first {MAX_SUFFIX} variants are all taken"),
        fix: "give this case a more distinctive name".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_readable_ascii_handle() {
        assert_eq!(slugify("Operation Lighthouse"), "operation-lighthouse");
        assert_eq!(slugify("  Spaced   Out  "), "spaced-out");
        assert_eq!(slugify("Acme Corp. (2019) — v2"), "acme-corp-2019-v2");
    }

    #[test]
    fn diacritics_fold_the_same_way_the_matcher_folds_them() {
        assert_eq!(slugify("Operacja Żółć"), slugify("Operacja Zolc"));
        assert_eq!(slugify("Łukasz"), "lukasz");
    }

    #[test]
    fn a_name_with_nothing_sluggable_in_it_still_gets_a_handle() {
        assert_eq!(slugify("!!!"), FALLBACK_SLUG);
        assert_eq!(slugify(""), FALLBACK_SLUG);
    }

    #[test]
    fn long_names_are_cut_to_a_word_boundary_and_never_end_in_a_hyphen() {
        let slug = slugify(&"word ".repeat(40));
        assert!(slug.chars().count() <= MAX_SLUG_LEN);
        assert!(!slug.ends_with('-'));

        let one_long_word = slugify(&"x".repeat(200));
        assert_eq!(one_long_word.chars().count(), MAX_SLUG_LEN);
    }
}
