//! Property-based tests for the invariants that must hold for *every* input,
//! not just the ones somebody thought to put in a table.
//!
//! Four claims are made here, and each is the kind of thing that breaks
//! silently:
//!
//! 1. Normalisation never panics, on any Unicode string at all.
//! 2. Normalisation is idempotent: normalising an already-normalised value
//!    changes nothing. If it were not, `vesica rescan-matches` would move the
//!    data every time it ran.
//! 3. The raw value comes back unchanged. It is the only copy of what a person
//!    actually typed.
//! 4. A partial date's lower bound is never after its upper bound.

use proptest::prelude::*;
use vesica_core::{DateStyle, NormalizeContext, PartialDate, PropertyType};

/// Contexts worth exercising: no region, and a couple of real ones, since the
/// region reaches phone parsing and date order.
fn contexts() -> Vec<NormalizeContext> {
    vec![
        NormalizeContext::new(),
        NormalizeContext::with_region("PL"),
        NormalizeContext::with_region("US"),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Arbitrary Unicode, every property type, every context: an `Err` is a
    /// fine answer, a panic is not.
    #[test]
    fn normalisation_never_panics_on_arbitrary_unicode(s in "\\PC{0,64}") {
        for ctx in contexts() {
            for ptype in PropertyType::ALL {
                let _ = ptype.normalize(&s, &ctx);
                let _ = ptype.validate(&s, &ctx);
                let _ = ptype.blocking_keys(&s, &ctx);
            }
        }
    }

    /// Whatever came in comes back out, byte for byte.
    #[test]
    fn the_raw_value_is_always_recoverable(s in "\\PC{0,64}") {
        for ctx in contexts() {
            for ptype in PropertyType::ALL {
                if let Ok(n) = ptype.normalize(&s, &ctx) {
                    prop_assert_eq!(&n.raw, &s, "{} lost the raw value", ptype);
                }
            }
        }
    }

    /// Normalising twice is the same as normalising once.
    #[test]
    fn normalisation_is_idempotent(s in "\\PC{0,64}") {
        for ctx in contexts() {
            for ptype in PropertyType::ALL {
                let Ok(first) = ptype.normalize(&s, &ctx) else {
                    continue;
                };
                let second = ptype.normalize(&first.value, &ctx);
                let Ok(second) = second else {
                    prop_assert!(
                        false,
                        "{} normalised {:?} to {:?}, which it then rejected",
                        ptype, s, first.value
                    );
                    continue;
                };
                prop_assert_eq!(
                    &second.value, &first.value,
                    "{} is not idempotent on {:?}", ptype, s
                );
                // Blocking keys are derived from the normalised value, so they
                // have to be stable across the second pass too.
                prop_assert_eq!(
                    second.blocking_keys(), first.blocking_keys(),
                    "{} produced unstable blocking keys for {:?}", ptype, s
                );
            }
        }
    }

    /// Structured date input: bounds are ordered, precision survives, and the
    /// canonical text re-parses to exactly the same date.
    #[test]
    fn partial_date_bounds_are_ordered(
        year in 1i32..=9999,
        month in 1u32..=12,
        day in 1u32..=31,
        hour in 0u32..=23,
        minute in 0u32..=59,
    ) {
        let candidates = [
            format!("{year:04}"),
            format!("{year:04}-{month:02}"),
            format!("{year:04}-{month:02}-{day:02}"),
            format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z"),
        ];
        for input in candidates {
            let Ok(parsed) = PartialDate::parse(&input, DateStyle::DayFirst) else {
                // e.g. the 31st of February: refused, which is correct.
                continue;
            };
            let d = parsed.date;
            prop_assert!(
                d.lower_bound() <= d.upper_bound(),
                "{:?} produced inverted bounds", input
            );
            let reparsed = PartialDate::parse(d.text(), DateStyle::DayFirst)
                .expect("the canonical form must re-parse")
                .date;
            prop_assert_eq!(&reparsed, &d, "canonical text of {:?} did not round-trip", input);
            prop_assert!(d.contains(&d));
            prop_assert!(d.overlaps(&d));
        }
    }

    /// Arbitrary text through the date parser: never a panic, and any date it
    /// does produce still has ordered bounds.
    #[test]
    fn arbitrary_text_never_produces_an_inverted_date(s in "\\PC{0,32}") {
        for style in [DateStyle::DayFirst, DateStyle::MonthFirst] {
            if let Ok(parsed) = PartialDate::parse(&s, style) {
                prop_assert!(parsed.date.lower_bound() <= parsed.date.upper_bound());
            }
        }
    }

    /// Folding is idempotent and total, which is what every comparison key
    /// downstream is built on.
    #[test]
    fn folding_is_idempotent(s in "\\PC{0,64}") {
        let folded = vesica_core::folding::fold_for_comparison(&s);
        prop_assert_eq!(
            vesica_core::folding::fold_for_comparison(&folded),
            folded.clone()
        );
        let translit = vesica_core::folding::transliterate(&s);
        prop_assert_eq!(
            vesica_core::folding::transliterate(&translit),
            translit.clone()
        );
        prop_assert!(translit.is_ascii());
        // The phonetic key is only ever built from ASCII, so it cannot panic
        // on a multi-byte boundary.
        let _ = vesica_core::folding::phonetic_key(&s);
    }
}
