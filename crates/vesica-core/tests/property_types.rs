//! Table-driven cases for every property type in `04-data-model.md` 4.1.1.
//!
//! Each table is a list of `(input, expected normalised value)` plus, where it
//! matters, the warnings the value should carry. The awkward inputs are the
//! point: transposed check digits, `+tag` local parts, tracking parameters,
//! diacritics, two-digit years.

use vesica_core::{
    ErrorCode, IdentifierScheme, MatchWeight, NormalizeContext, Normalized, PropertyType,
    ValidationError, WarningCode,
};

fn plain() -> NormalizeContext {
    NormalizeContext::new()
}

fn pl() -> NormalizeContext {
    NormalizeContext::with_region("PL")
}

#[track_caller]
fn norm(ptype: PropertyType, raw: &str, ctx: &NormalizeContext) -> Normalized {
    match ptype.normalize(raw, ctx) {
        Ok(n) => {
            assert_eq!(n.raw, raw, "raw value must come back unchanged");
            n
        }
        Err(e) => panic!(
            "{ptype} should accept {raw:?} but returned {e} (fix: {})",
            e.fix
        ),
    }
}

#[track_caller]
fn expect_err(ptype: PropertyType, raw: &str, ctx: &NormalizeContext) -> ValidationError {
    match ptype.normalize(raw, ctx) {
        Ok(n) => panic!("{ptype} should reject {raw:?} but produced {:?}", n.value),
        Err(e) => {
            assert!(!e.fix.is_empty(), "every error must carry a fix string");
            e
        }
    }
}

#[track_caller]
fn assert_codes(n: &Normalized, expected: &[WarningCode]) {
    let got: Vec<WarningCode> = n.warnings.iter().map(|w| w.code).collect();
    assert_eq!(got, expected, "warnings for {:?}: {:?}", n.raw, n.warnings);
}

#[track_caller]
fn table(ptype: PropertyType, ctx: &NormalizeContext, cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        assert_eq!(
            norm(ptype, input, ctx).value,
            *expected,
            "{ptype} normalising {input:?}"
        );
    }
}

// --- names ---------------------------------------------------------------

#[test]
fn names_fold_without_the_stored_value_losing_diacritics() {
    table(
        PropertyType::Name,
        &plain(),
        &[
            ("Jan Kowalski", "jan kowalski"),
            ("  JAN   KOWALSKI  ", "jan kowalski"),
            ("Kowalski, Jan", "kowalski jan"),
            ("Jean-Luc O'Brien", "jean luc o brien"),
            // NFKD decomposes ó/ż/ć; ł has no decomposition and survives here.
            ("Łukasz Żółć", "łukasz zołc"),
            ("Ｊａｎ Ｋｏｗａｌｓｋｉ", "jan kowalski"), // NFKD fullwidth fold
            ("Оле́г", "олег"),
        ],
    );

    let n = norm(PropertyType::Name, "Łukasz Żółć", &plain());
    assert_eq!(
        n.raw, "Łukasz Żółć",
        "the stored value keeps its diacritics"
    );
    assert_eq!(n.aux("translit"), Some("lukasz zolc"));
    assert_eq!(PropertyType::Name.match_weight(), MatchWeight::Medium);
}

#[test]
fn transliterated_and_reordered_names_meet_in_the_same_bucket() {
    let keys = |s: &str| norm(PropertyType::Name, s, &plain()).blocking_keys();
    assert_eq!(keys("Łukasz Żółć"), keys("Zolc, Lukasz"));
    assert_eq!(keys("Jan Kowalski"), keys("KOWALSKI JAN"));
    assert_ne!(keys("Jan Kowalski"), keys("Anna Nowak"));
}

#[test]
fn a_name_of_pure_punctuation_is_an_error() {
    assert_eq!(
        expect_err(PropertyType::Name, "!!!", &plain()).code,
        ErrorCode::Malformed
    );
    assert_eq!(
        expect_err(PropertyType::Name, "   ", &plain()).code,
        ErrorCode::Empty
    );
}

// --- emails --------------------------------------------------------------

#[test]
fn emails_fold_tags_everywhere_and_dots_only_at_google() {
    table(
        PropertyType::Email,
        &plain(),
        &[
            ("J.Kowalski@Example.COM", "j.kowalski@example.com"),
            ("J.Kowalski+lek@Gmail.COM", "jkowalski@gmail.com"),
            ("jan.kowalski@googlemail.com", "jankowalski@gmail.com"),
            // The dot rule is Google's alone: these two are different people.
            ("j.kowalski@example.com", "j.kowalski@example.com"),
            ("jkowalski@example.com", "jkowalski@example.com"),
            ("jan+news@fastmail.com", "jan@fastmail.com"),
            ("user+tag+more@example.com", "user@example.com"),
            // A leading `+` is part of the address, not a tag separator.
            ("+plus@example.com", "+plus@example.com"),
            ("jan@Example.COM.", "jan@example.com"),
            ("jan@bücher.de", "jan@xn--bcher-kva.de"),
            // Quoted local parts are left exactly alone.
            (r#""a.b+c"@Example.com"#, r#""a.b+c"@example.com"#),
        ],
    );
    assert_eq!(
        norm(PropertyType::Email, "J.Kowalski+lek@Gmail.COM", &plain()).aux("tag"),
        Some("lek")
    );
    assert_eq!(PropertyType::Email.match_weight(), MatchWeight::Strong);
}

#[test]
fn malformed_emails_are_errors_with_a_fix() {
    for bad in [
        "jan",
        "@example.com",
        "jan@",
        "jan@localhost",
        "jan doe@example.com",
        "",
    ] {
        let e = expect_err(PropertyType::Email, bad, &plain());
        assert!(e.fix.contains('@'), "fix should show the shape: {}", e.fix);
    }
}

// --- phones --------------------------------------------------------------

#[test]
fn phones_reach_e164_through_the_case_region() {
    table(
        PropertyType::Phone,
        &pl(),
        &[
            ("601 234 567", "+48601234567"),
            ("601234567", "+48601234567"),
            // The trunk `0` an analyst copies off an old document.
            ("0601 234 567", "+48601234567"),
            ("+48 601 234 567", "+48601234567"),
            ("+48 (601) 234-567", "+48601234567"),
        ],
    );
    table(
        PropertyType::Phone,
        &NormalizeContext::with_region("US"),
        &[
            ("(202) 555-0181", "+12025550181"),
            ("202-555-0181", "+12025550181"),
        ],
    );
    table(
        PropertyType::Phone,
        &NormalizeContext::with_region("GB"),
        &[("020 7946 0958", "+442079460958")],
    );
    assert_eq!(PropertyType::Phone.match_weight(), MatchWeight::Strong);
}

#[test]
fn international_input_is_unaffected_by_the_region() {
    let number = "+48 601 234 567";
    let expected = "+48601234567";
    for region in ["PL", "US", "GB", "DE", "JP"] {
        let n = norm(
            PropertyType::Phone,
            number,
            &NormalizeContext::with_region(region),
        );
        assert_eq!(n.value, expected, "region {region} changed an E.164 number");
        assert_eq!(n.aux("country"), Some("PL"));
    }
    // Even with no region at all.
    assert_eq!(norm(PropertyType::Phone, number, &plain()).value, expected);
}

#[test]
fn a_national_number_without_a_region_says_what_is_missing() {
    let e = expect_err(PropertyType::Phone, "601 234 567", &plain());
    assert_eq!(e.code, ErrorCode::MissingContext);
    assert!(e.fix.contains("default region"), "fix: {}", e.fix);
}

// --- urls and domains ----------------------------------------------------

#[test]
fn urls_fold_scheme_and_host_and_drop_tracking_parameters() {
    table(
        PropertyType::Url,
        &plain(),
        &[
            ("https://Example.com/Path/", "https://example.com/Path/"),
            ("HTTPS://EXAMPLE.COM:443/", "https://example.com"),
            ("http://example.com:80/a", "http://example.com/a"),
            (
                "https://example.com/a?utm_source=news&utm_medium=email&id=7",
                "https://example.com/a?id=7",
            ),
            (
                "https://example.com/a?fbclid=XYZ&gclid=ABC",
                "https://example.com/a",
            ),
            ("https://example.com/a#section-3", "https://example.com/a"),
            ("https://bücher.de/x", "https://xn--bcher-kva.de/x"),
        ],
    );
    // The path keeps its case: paths are case-sensitive, hosts are not.
    let n = norm(PropertyType::Url, "https://Example.com/Path", &plain());
    assert!(n.value.ends_with("/Path"));
    assert_eq!(n.aux("registrable"), Some("example.com"));
}

#[test]
fn a_url_without_a_scheme_is_accepted_and_flagged() {
    let n = norm(PropertyType::Url, "example.com/a", &plain());
    assert_eq!(n.value, "https://example.com/a");
    assert_codes(&n, &[WarningCode::UrlSchemeAssumed]);
}

#[test]
fn domains_are_punycode_aware_and_keep_a_registrable_form() {
    table(
        PropertyType::Domain,
        &plain(),
        &[
            ("Example.COM.", "example.com"),
            ("bücher.de", "xn--bcher-kva.de"),
            ("XN--BCHER-KVA.DE", "xn--bcher-kva.de"),
            ("mail.example.co.uk", "mail.example.co.uk"),
        ],
    );
    assert_eq!(
        norm(PropertyType::Domain, "mail.example.co.uk", &plain()).aux("registrable"),
        Some("example.co.uk"),
        "the registrable domain must respect the public suffix list"
    );
    for bad in ["", "http://example.com", "jan@example.com", "exa mple.com"] {
        expect_err(PropertyType::Domain, bad, &plain());
    }
    assert_eq!(PropertyType::Domain.match_weight(), MatchWeight::Strong);
}

// --- identifiers ---------------------------------------------------------

#[test]
fn identifiers_compact_and_verify_where_the_scheme_allows() {
    let cases: &[(&str, &str, &str)] = &[
        ("NIP 525-224-84-81", "5252248481", "nip"),
        ("5252248481", "5252248481", "nip"),
        ("44051401359", "44051401359", "pesel"),
        ("PESEL: 44051401359", "44051401359", "pesel"),
        ("123456785", "123456785", "regon"),
        ("12345678512347", "12345678512347", "regon"),
        ("1M8GDM9AXKP042788", "1M8GDM9AXKP042788", "vin"),
        (
            "PL61 1090 1014 0000 0712 1981 2874",
            "PL61109010140000071219812874",
            "iban",
        ),
    ];
    for (input, value, scheme) in cases {
        let n = norm(PropertyType::Identifier, input, &plain());
        assert_eq!(n.value, *value, "normalising {input:?}");
        assert_eq!(n.aux("scheme"), Some(*scheme), "scheme for {input:?}");
        assert_codes(&n, &[]);
    }
    assert_eq!(
        PropertyType::Identifier.match_weight(),
        MatchWeight::VeryStrong
    );
}

#[test]
fn a_failed_checksum_is_a_warning_and_the_value_is_still_usable() {
    // One transposed digit in each scheme.
    let corrupted: &[(&str, &str)] = &[
        ("NIP 5252248418", "5252248418"),
        ("PESEL 44051401358", "44051401358"),
        ("REGON 123456789", "123456789"),
        ("VIN 1M8GDM9A1KP042788", "1M8GDM9A1KP042788"),
        (
            "IBAN PL61 1090 1014 0000 0712 1981 2847",
            "PL61109010140000071219812847",
        ),
    ];
    for (input, value) in corrupted {
        let n = norm(PropertyType::Identifier, input, &plain());
        assert_eq!(n.value, *value, "the value is stored exactly as given");
        assert_eq!(n.raw, *input, "the raw value is never discarded");
        assert_codes(&n, &[WarningCode::ChecksumFailed]);
        assert!(n.has_warnings());
        // Still a usable value: validation passes, and it still blocks.
        assert!(
            PropertyType::Identifier.validate(input, &plain()).is_ok(),
            "a bad check digit must not make the value unusable"
        );
        assert!(!n.blocking_keys().is_empty());
    }
}

#[test]
fn a_guessed_scheme_says_so_when_its_checksum_fails() {
    // No label: the shape says NIP, and the guess may simply be wrong.
    let n = norm(PropertyType::Identifier, "5252248418", &plain());
    assert_codes(
        &n,
        &[WarningCode::ChecksumFailed, WarningCode::SchemeInferred],
    );
    // Labelled: the analyst asserted the scheme, so only the failure is
    // reported.
    let n = norm(PropertyType::Identifier, "NIP 5252248418", &plain());
    assert_codes(&n, &[WarningCode::ChecksumFailed]);
}

#[test]
fn an_identifier_in_no_known_scheme_normalises_quietly() {
    for input in ["AB1234567", "passport EK 4472119", "X-999"] {
        let n = norm(PropertyType::Identifier, input, &plain());
        assert_codes(&n, &[]);
        assert_eq!(n.aux("scheme"), None);
    }
    assert_eq!(
        norm(PropertyType::Identifier, "passport EK 4472119", &plain()).value,
        "PASSPORTEK4472119"
    );
}

#[test]
fn identifier_scheme_inference_is_directly_testable() {
    assert_eq!(
        IdentifierScheme::infer("5252248481"),
        Some(IdentifierScheme::Nip)
    );
    assert_eq!(
        IdentifierScheme::infer("44051401359"),
        Some(IdentifierScheme::Pesel)
    );
    assert_eq!(
        IdentifierScheme::infer("1M8GDM9AXKP042788"),
        Some(IdentifierScheme::Vin)
    );
    assert_eq!(IdentifierScheme::infer("AB1234567"), None);
    assert_eq!(
        IdentifierScheme::parse_label("nip"),
        Some(IdentifierScheme::Nip)
    );
    assert!(IdentifierScheme::Nip.is_valid("5252248481"));
}

// --- iban, wallet, checksum ---------------------------------------------

#[test]
fn ibans_uppercase_compact_and_verify() {
    let n = norm(
        PropertyType::Iban,
        "pl61 1090 1014 0000 0712 1981 2874",
        &plain(),
    );
    assert_eq!(n.value, "PL61109010140000071219812874");
    assert_codes(&n, &[]);

    let bad = norm(PropertyType::Iban, "GB82 WEST 1234 5698 7654 31", &plain());
    assert_codes(&bad, &[WarningCode::ChecksumFailed]);
    assert_eq!(bad.value, "GB82WEST12345698765431");

    assert_eq!(
        expect_err(PropertyType::Iban, "PL", &plain()).code,
        ErrorCode::OutOfRange
    );
}

#[test]
fn wallets_keep_case_where_case_is_meaningful() {
    // bech32 is case-insensitive; canonical form is lower case.
    let n = norm(
        PropertyType::Wallet,
        "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
        &plain(),
    );
    assert_eq!(n.value, "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4");
    assert_eq!(n.aux("scheme"), Some("bech32"));
    assert_codes(&n, &[]);

    // A corrupted bech32 address is flagged, not refused.
    let bad = norm(
        PropertyType::Wallet,
        "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5",
        &plain(),
    );
    assert_codes(&bad, &[WarningCode::ChecksumFailed]);

    // EVM addresses fold case; the EIP-55 mixed case is not a checksum this
    // build can verify, so it is not claimed to be one.
    let evm = norm(
        PropertyType::Wallet,
        "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B",
        &plain(),
    );
    assert_eq!(evm.value, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
    assert_eq!(evm.aux("scheme"), Some("evm"));

    // A legacy base58 address keeps its case, because base58 is
    // case-sensitive.
    let legacy = norm(
        PropertyType::Wallet,
        "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
        &plain(),
    );
    assert_eq!(legacy.value, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa");
    assert_eq!(legacy.aux("scheme"), Some("base58"));
}

#[test]
fn checksums_lowercase_and_check_their_length() {
    let n = norm(
        PropertyType::Checksum,
        "SHA256:E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
        &plain(),
    );
    assert_eq!(
        n.value,
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_codes(&n, &[]);

    // No prefix: inferred from the length, and said so.
    let bare = norm(
        PropertyType::Checksum,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        &plain(),
    );
    assert_codes(&bare, &[WarningCode::DigestAlgorithmInferred]);
    assert_eq!(bare.aux("algorithm"), Some("sha256"));

    // Wrong length for the algorithm it claims: stored, flagged.
    let short = norm(PropertyType::Checksum, "sha256:abcdef", &plain());
    assert_codes(&short, &[WarningCode::DigestLengthMismatch]);

    expect_err(PropertyType::Checksum, "sha256:nothex", &plain());
    assert_eq!(
        PropertyType::Checksum.match_weight(),
        MatchWeight::VeryStrong
    );
}

// --- the remaining types -------------------------------------------------

#[test]
fn ips_reach_their_canonical_form() {
    table(
        PropertyType::Ip,
        &plain(),
        &[
            ("2001:0DB8:0000:0000:0000:0000:0000:0001", "2001:db8::1"),
            ("[::1]", "::1"),
            ("192.0.2.4", "192.0.2.4"),
            ("  2001:db8::1  ", "2001:db8::1"),
        ],
    );
    for bad in ["", "999.0.0.1", "not an ip"] {
        expect_err(PropertyType::Ip, bad, &plain());
    }
}

#[test]
fn countries_reach_alpha_two() {
    table(
        PropertyType::Country,
        &plain(),
        &[
            ("PL", "PL"),
            ("pl", "PL"),
            ("POL", "PL"),
            ("616", "PL"),
            ("Poland", "PL"),
            ("Polska", "PL"),
            ("  polska  ", "PL"),
            ("United Kingdom", "GB"),
            ("USA", "US"),
        ],
    );
    let e = expect_err(PropertyType::Country, "Atlantis", &plain());
    assert_eq!(e.code, ErrorCode::UnknownValue);
    assert!(e.fix.contains("alpha-2"), "fix: {}", e.fix);
    assert_eq!(PropertyType::Country.match_weight(), MatchWeight::Weak);
}

#[test]
fn addresses_yield_a_postcode_and_a_house_number() {
    let n = norm(
        PropertyType::Address,
        "ul. Piękna 12/4, 00-549 Warszawa",
        &plain(),
    );
    assert_eq!(n.value, "ul piekna 12 4 00 549 warszawa");
    assert_eq!(n.aux("postcode"), Some("00-549"));
    assert_eq!(n.aux("house"), Some("12/4"));

    let us = norm(
        PropertyType::Address,
        "1600 Pennsylvania Ave NW, Washington, DC 20500",
        &plain(),
    );
    assert_eq!(us.aux("postcode"), Some("20500"));
    assert_eq!(us.aux("house"), Some("1600"));

    let plain_addr = norm(PropertyType::Address, "somewhere in the woods", &plain());
    assert_eq!(plain_addr.aux("postcode"), None);
    expect_err(PropertyType::Address, "   ", &plain());
}

#[test]
fn text_is_trimmed_and_nothing_else() {
    table(
        PropertyType::Text,
        &plain(),
        &[
            ("  He said \"no\".  ", "He said \"no\"."),
            ("Łódź", "Łódź"),
            ("", ""),
        ],
    );
    assert_eq!(PropertyType::Text.match_weight(), MatchWeight::None);
    assert!(
        norm(PropertyType::Text, "anything", &plain())
            .blocking_keys()
            .is_empty()
    );
}

#[test]
fn numbers_tolerate_european_separators() {
    table(
        PropertyType::Number,
        &plain(),
        &[
            ("42.5", "42.5"),
            ("42,5", "42.5"),
            ("42", "42"),
            ("42.0", "42"),
            ("-3.25", "-3.25"),
            ("1 234,56", "1234.56"),
            ("1,234", "1234"),
        ],
    );
    assert_eq!(
        norm(PropertyType::Number, "42,5", &plain()).numeric,
        Some(42.5)
    );
    for bad in ["", "forty two", "inf", "NaN"] {
        expect_err(PropertyType::Number, bad, &plain());
    }
}

#[test]
fn entity_references_must_be_ulids() {
    table(
        PropertyType::Entity,
        &plain(),
        &[
            ("01ARZ3NDEKTSV4RRFFQ69G5FAV", "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            ("01arz3ndektsv4rrffq69g5fav", "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        ],
    );
    for bad in [
        "",
        "not-a-ulid",
        "01ARZ3NDEKTSV4RRFFQ69G5FA",
        "ZZARZ3NDEKTSV4RRFFQ69G5FAV",
    ] {
        expect_err(PropertyType::Entity, bad, &plain());
    }
}

// --- the type table itself ----------------------------------------------

#[test]
fn every_property_type_round_trips_through_its_name() {
    assert_eq!(PropertyType::ALL.len(), 16);
    for ptype in PropertyType::ALL {
        let s = ptype.as_str();
        assert_eq!(s.parse::<PropertyType>().expect("known type"), ptype);
    }
    assert!("nonsense".parse::<PropertyType>().is_err());
}

#[test]
fn match_weights_match_the_data_model_table() {
    use MatchWeight::{Medium, None as NoWeight, Strong, VeryStrong, Weak};
    let expected = [
        (PropertyType::Name, Medium),
        (PropertyType::Email, Strong),
        (PropertyType::Phone, Strong),
        (PropertyType::Url, Medium),
        (PropertyType::Domain, Strong),
        (PropertyType::Ip, Strong),
        (PropertyType::Identifier, VeryStrong),
        (PropertyType::Date, Medium),
        (PropertyType::Country, Weak),
        (PropertyType::Address, Medium),
        (PropertyType::Text, NoWeight),
        (PropertyType::Number, Weak),
        (PropertyType::Entity, NoWeight),
        (PropertyType::Checksum, VeryStrong),
        (PropertyType::Iban, VeryStrong),
        (PropertyType::Wallet, VeryStrong),
    ];
    for (ptype, weight) in expected {
        assert_eq!(ptype.match_weight(), weight, "{ptype}");
    }
}

// --- regressions ---------------------------------------------------------

#[test]
fn a_bare_combining_mark_does_not_desynchronise_a_name_from_its_transliteration() {
    // Found by the idempotence property test. `\u{0ac7}` is a Gujarati vowel
    // sign: NFKD folding drops it as a combining mark, while `deunicode`
    // transliterates it to the letter `e`. Deriving the transliteration from
    // the raw input therefore produced a two-letter key for a one-letter name,
    // and re-normalising the stored value produced a different key again.
    let ctx = plain();
    let first = norm(PropertyType::Name, "\u{0ac7}A", &ctx);
    assert_eq!(first.value, "a");
    assert_eq!(first.aux("translit"), Some("a"));
    let second = norm(PropertyType::Name, &first.value, &ctx);
    assert_eq!(first.blocking_keys(), second.blocking_keys());
}
