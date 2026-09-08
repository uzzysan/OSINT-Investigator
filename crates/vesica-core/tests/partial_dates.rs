//! Partial dates: precision is preserved, bounds are honest, and nothing
//! anywhere widens a year into 1 January.

use vesica_core::{
    DateOrder, DatePrecision, DateStyle, ErrorCode, NormalizeContext, PartialDate, PropertyType,
    WarningCode,
};

#[track_caller]
fn parse(input: &str) -> PartialDate {
    PartialDate::parse(input, DateStyle::DayFirst)
        .unwrap_or_else(|e| panic!("{input:?} should parse: {e} (fix: {})", e.fix()))
        .date
}

#[test]
fn iso_prefixes_keep_their_precision() {
    let cases: &[(&str, DatePrecision, &str)] = &[
        ("1978", DatePrecision::Year, "1978"),
        ("1978-04", DatePrecision::Month, "1978-04"),
        ("1978-04-12", DatePrecision::Day, "1978-04-12"),
        (
            "1978-04-12T09:00:00Z",
            DatePrecision::Instant,
            "1978-04-12T09:00:00Z",
        ),
        ("2019", DatePrecision::Year, "2019"),
        ("2019-3", DatePrecision::Month, "2019-03"),
        ("2019-3-4", DatePrecision::Day, "2019-03-04"),
    ];
    for (input, precision, text) in cases {
        let d = parse(input);
        assert_eq!(d.precision(), *precision, "precision of {input:?}");
        assert_eq!(d.text(), *text, "canonical text of {input:?}");
    }
}

#[test]
fn bounds_span_exactly_the_period_that_was_known() {
    let year = parse("2019");
    assert_eq!(year.lower_iso(), "2019-01-01T00:00:00Z");
    assert!(year.upper_iso().starts_with("2019-12-31T23:59:59"));

    let month = parse("2019-02");
    assert_eq!(month.lower_iso(), "2019-02-01T00:00:00Z");
    assert!(month.upper_iso().starts_with("2019-02-28T23:59:59"));

    // A leap February really is a day longer.
    let leap = parse("2020-02");
    assert!(leap.upper_iso().starts_with("2020-02-29T23:59:59"));

    let day = parse("2019-03-14");
    assert_eq!(day.lower_iso(), "2019-03-14T00:00:00Z");
    assert!(day.upper_iso().starts_with("2019-03-14T23:59:59"));

    let instant = parse("1978-04-12T09:00:00Z");
    assert_eq!(instant.lower_bound(), instant.upper_bound());
}

#[test]
fn a_year_is_never_a_point_in_time() {
    let year = parse("2019");
    let jan_first = parse("2019-01-01");
    assert_ne!(year, jan_first);
    assert_ne!(year.upper_bound(), jan_first.upper_bound());
    assert_eq!(year.precision(), DatePrecision::Year);
    // The year contains 1 January but is not equal to it, and the type offers
    // no way to collapse it to one.
    assert!(year.contains(&jan_first));
    assert!(!jan_first.contains(&year));
}

#[test]
fn the_awkward_dates() {
    use vesica_core::DateError;

    // 2019 was not a leap year.
    assert!(matches!(
        PartialDate::parse("2019-02-29", DateStyle::DayFirst),
        Err(DateError::DayOutOfRange {
            year: 2019,
            month: 2,
            day: 29
        })
    ));
    // 2020 was.
    assert_eq!(parse("2020-02-29").text(), "2020-02-29");

    assert!(matches!(
        PartialDate::parse("2019-13", DateStyle::DayFirst),
        Err(DateError::MonthOutOfRange { month: 13 })
    ));
    assert!(matches!(
        PartialDate::parse("2019-00", DateStyle::DayFirst),
        Err(DateError::MonthOutOfRange { month: 0 })
    ));
    assert!(matches!(
        PartialDate::parse("2019-04-31", DateStyle::DayFirst),
        Err(DateError::DayOutOfRange { .. })
    ));
    // Two-digit years are refused rather than guessed at.
    for ambiguous in ["78", "78-04", "12/04/78"] {
        assert!(
            matches!(
                PartialDate::parse(ambiguous, DateStyle::DayFirst),
                Err(DateError::TwoDigitYear { .. })
            ),
            "{ambiguous} should be refused as ambiguous"
        );
    }
    for nonsense in ["", "   ", "yesterday", "2019-", "-2019", "20190314"] {
        assert!(
            PartialDate::parse(nonsense, DateStyle::DayFirst).is_err(),
            "{nonsense:?} should not parse"
        );
    }
    // Every error explains how to write a valid value.
    let e = PartialDate::parse("78", DateStyle::DayFirst).expect_err("ambiguous");
    assert!(e.fix().contains("four-digit"), "fix: {}", e.fix());
}

#[test]
fn timestamps_and_offsets() {
    assert_eq!(parse("1978-04-12T09:00:00Z").text(), "1978-04-12T09:00:00Z");
    // An offset is converted to UTC, not dropped.
    assert_eq!(
        parse("1978-04-12T11:00:00+02:00").text(),
        "1978-04-12T09:00:00Z"
    );
    assert_eq!(
        parse("1978-04-12T09:00:00.500Z").lower_bound(),
        parse("1978-04-12T09:00:00.500Z").upper_bound()
    );

    // No offset: read as UTC, and said so.
    let parsed = PartialDate::parse("1978-04-12 09:00:00", DateStyle::DayFirst)
        .expect("a naive timestamp is still a timestamp");
    assert_eq!(parsed.date.text(), "1978-04-12T09:00:00Z");
    assert_eq!(
        parsed.warnings.iter().map(|w| w.code).collect::<Vec<_>>(),
        vec![WarningCode::TimezoneAssumed]
    );
}

#[test]
fn slash_dates_follow_the_case_region_and_say_when_they_guessed() {
    // Unambiguous: 25 cannot be a month, whatever the region.
    for style in [DateStyle::DayFirst, DateStyle::MonthFirst] {
        let p = PartialDate::parse("25/12/2019", style).expect("unambiguous");
        assert_eq!(p.date.text(), "2019-12-25");
        assert!(p.warnings.is_empty(), "nothing was guessed");
    }

    // Ambiguous: both readings are calendrically possible.
    let day_first = PartialDate::parse("12/04/1978", DateStyle::DayFirst).expect("day first");
    assert_eq!(day_first.date.text(), "1978-04-12");
    assert_eq!(
        day_first.warnings.first().map(|w| w.code),
        Some(WarningCode::AmbiguousDateOrder)
    );

    let month_first = PartialDate::parse("12/04/1978", DateStyle::MonthFirst).expect("month first");
    assert_eq!(month_first.date.text(), "1978-12-04");

    // Dotted and year-first forms.
    assert_eq!(parse("12.04.1978").text(), "1978-04-12");
    assert_eq!(parse("1978/04/12").text(), "1978-04-12");
    assert_eq!(parse("04/1978").precision(), DatePrecision::Month);
}

#[test]
fn the_case_region_picks_the_date_style() {
    assert_eq!(DateStyle::for_region(Some("PL")), DateStyle::DayFirst);
    assert_eq!(DateStyle::for_region(Some("us")), DateStyle::MonthFirst);
    assert_eq!(DateStyle::for_region(None), DateStyle::DayFirst);
    assert_eq!(
        NormalizeContext::with_region("US").date_style(),
        DateStyle::MonthFirst
    );
    // Through the property type, end to end.
    let pl = NormalizeContext::with_region("PL");
    let us = NormalizeContext::with_region("US");
    assert_eq!(
        PropertyType::Date
            .normalize("12/04/1978", &pl)
            .expect("pl")
            .value,
        "1978-04-12"
    );
    assert_eq!(
        PropertyType::Date
            .normalize("12/04/1978", &us)
            .expect("us")
            .value,
        "1978-12-04"
    );
}

#[test]
fn comparison_refuses_to_order_overlapping_ranges() {
    let year = parse("2019");
    let day = parse("2019-03-14");
    let earlier = parse("2018-12-31");
    let later = parse("2020-01-01");

    assert_eq!(year.compare(&day), DateOrder::Overlapping);
    assert_eq!(day.compare(&year), DateOrder::Overlapping);
    assert_eq!(earlier.compare(&year), DateOrder::Before);
    assert_eq!(later.compare(&year), DateOrder::After);
    assert!(year.overlaps(&day));
    assert!(!year.overlaps(&later));
}

#[test]
fn blocking_keys_let_a_year_meet_a_day() {
    let ctx = NormalizeContext::new();
    let keys = |s: &str| {
        PropertyType::Date
            .blocking_keys(s, &ctx)
            .expect("parses")
            .into_iter()
            .map(|k| k.key)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys("1978"), vec!["1978"]);
    assert_eq!(keys("1978-04"), vec!["1978", "1978-04"]);
    assert_eq!(keys("1978-04-12"), vec!["1978", "1978-04", "1978-04-12"]);
    // A year-precision birth date and a day-precision one share a bucket.
    let coarse = keys("1978");
    let fine = keys("1978-04-12");
    assert!(fine.iter().any(|k| coarse.contains(k)));
}

#[test]
fn the_date_property_type_reports_errors_with_a_fix() {
    let ctx = NormalizeContext::new();
    let e = PropertyType::Date
        .normalize("2019-02-29", &ctx)
        .expect_err("impossible day");
    assert_eq!(e.code, ErrorCode::OutOfRange);
    assert!(!e.fix.is_empty());

    let e = PropertyType::Date
        .normalize("78", &ctx)
        .expect_err("two-digit year");
    assert_eq!(e.code, ErrorCode::Ambiguous);
}

#[test]
fn stored_columns_round_trip() {
    let d = parse("2019-03");
    let back = PartialDate::from_bounds(d.text(), d.precision(), d.lower_bound(), d.upper_bound())
        .expect("valid bounds");
    assert_eq!(d, back);

    // The one constructor that takes bounds refuses inverted ones.
    assert!(
        PartialDate::from_bounds(
            "2019",
            DatePrecision::Year,
            d.upper_bound(),
            d.lower_bound()
        )
        .is_err()
    );
}
