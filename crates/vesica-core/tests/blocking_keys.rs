//! Blocking-key derivation, one case per property type.
//!
//! Nothing consumes these yet — the resolver arrives in M5. They are tested
//! now because the knowledge of what makes two values worth comparing lives
//! here, and an untested definition is a definition that will have quietly
//! rotted by the time something reads it.

use vesica_core::{BlockingKey, BlockingKeyKind, NormalizeContext, PropertyType};

#[track_caller]
fn keys(ptype: PropertyType, raw: &str, ctx: &NormalizeContext) -> Vec<BlockingKey> {
    ptype
        .blocking_keys(raw, ctx)
        .unwrap_or_else(|e| panic!("{ptype} should accept {raw:?}: {e}"))
}

#[track_caller]
fn keys_of_kind(
    ptype: PropertyType,
    raw: &str,
    ctx: &NormalizeContext,
    kind: BlockingKeyKind,
) -> Vec<String> {
    keys(ptype, raw, ctx)
        .into_iter()
        .filter(|k| k.kind == kind)
        .map(|k| k.key)
        .collect()
}

fn plain() -> NormalizeContext {
    NormalizeContext::new()
}

#[test]
fn names_block_on_sorted_tokens_and_on_sound() {
    let ctx = plain();
    let sorted = keys_of_kind(
        PropertyType::Name,
        "Kowalski, Jan",
        &ctx,
        BlockingKeyKind::NameSorted,
    );
    assert_eq!(sorted, vec!["jan kowalski"]);
    assert_eq!(
        sorted,
        keys_of_kind(
            PropertyType::Name,
            "Jan Kowalski",
            &ctx,
            BlockingKeyKind::NameSorted
        )
    );

    let phonetic = keys_of_kind(
        PropertyType::Name,
        "Jan Kowalski",
        &ctx,
        BlockingKeyKind::NamePhonetic,
    );
    assert_eq!(phonetic.len(), 1);
    assert!(!phonetic[0].is_empty());
    // A spelling variant that sounds the same lands in the same bucket.
    assert_eq!(
        phonetic,
        keys_of_kind(
            PropertyType::Name,
            "Jan Kowalsky",
            &ctx,
            BlockingKeyKind::NamePhonetic
        )
    );
}

#[test]
fn identifiers_block_on_a_scheme_qualified_key() {
    let ctx = plain();
    assert_eq!(
        keys_of_kind(
            PropertyType::Identifier,
            "NIP 525-224-84-81",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["nip:5252248481"]
    );
    // The same number typed without its label reaches the same key, which is
    // the entire point of the scheme prefix being derived rather than typed.
    assert_eq!(
        keys_of_kind(
            PropertyType::Identifier,
            "5252248481",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["nip:5252248481"]
    );
    assert_eq!(
        keys_of_kind(
            PropertyType::Identifier,
            "AB1234567",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["id:AB1234567"]
    );
    assert_eq!(
        keys_of_kind(
            PropertyType::Iban,
            "pl61 1090 1014 0000 0712 1981 2874",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["iban:PL61109010140000071219812874"]
    );
    assert_eq!(
        keys_of_kind(
            PropertyType::Checksum,
            "SHA256:E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["digest:sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"]
    );
    assert_eq!(
        keys_of_kind(
            PropertyType::Ip,
            "2001:0DB8::1",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["ip:2001:db8::1"]
    );
}

#[test]
fn a_bad_check_digit_still_produces_a_blocking_key() {
    // Two people who both transcribed the same typo must still meet.
    let ctx = plain();
    assert_eq!(
        keys_of_kind(
            PropertyType::Identifier,
            "NIP 5252248418",
            &ctx,
            BlockingKeyKind::Ident
        ),
        vec!["nip:5252248418"]
    );
}

#[test]
fn phones_and_emails_block_on_their_normalised_form() {
    let pl = NormalizeContext::with_region("PL");
    assert_eq!(
        keys_of_kind(
            PropertyType::Phone,
            "0601 234 567",
            &pl,
            BlockingKeyKind::Phone
        ),
        vec!["+48601234567"]
    );
    assert_eq!(
        keys_of_kind(
            PropertyType::Email,
            "J.Kowalski+lek@Gmail.COM",
            &plain(),
            BlockingKeyKind::Email
        ),
        vec!["jkowalski@gmail.com"]
    );
    // The email domain alone is deliberately not a key: it would put every
    // Gmail user in one bucket.
    assert!(
        keys(PropertyType::Email, "jan@gmail.com", &plain())
            .iter()
            .all(|k| k.kind == BlockingKeyKind::Email)
    );
}

#[test]
fn urls_and_domains_block_on_the_registrable_domain() {
    let ctx = plain();
    let mut url_keys = keys_of_kind(
        PropertyType::Url,
        "https://mail.example.co.uk/a?utm_source=x",
        &ctx,
        BlockingKeyKind::Domain,
    );
    url_keys.sort();
    assert_eq!(url_keys, vec!["example.co.uk", "mail.example.co.uk"]);

    let mut domain_keys = keys_of_kind(
        PropertyType::Domain,
        "MAIL.Example.co.uk.",
        &ctx,
        BlockingKeyKind::Domain,
    );
    domain_keys.sort();
    assert_eq!(domain_keys, url_keys);
}

#[test]
fn addresses_block_on_postcode_house_and_token_bag() {
    let ctx = plain();
    let mut addr = keys_of_kind(
        PropertyType::Address,
        "ul. Piękna 12/4, 00-549 Warszawa",
        &ctx,
        BlockingKeyKind::AddrKey,
    );
    addr.sort();
    assert!(addr.contains(&"00-549".to_string()));
    assert!(addr.contains(&"00-549/12/4".to_string()));
    // The token bag is order-insensitive and transliterated.
    let other = keys_of_kind(
        PropertyType::Address,
        "00-549 Warszawa, Piekna 12/4 ul.",
        &ctx,
        BlockingKeyKind::AddrKey,
    );
    assert!(
        other.iter().any(|k| addr.contains(k)),
        "reordered address should share a key: {addr:?} vs {other:?}"
    );
}

#[test]
fn types_that_carry_no_evidence_produce_no_keys() {
    let ctx = plain();
    for (ptype, raw) in [
        (PropertyType::Text, "some free prose"),
        (PropertyType::Number, "42.5"),
        (PropertyType::Country, "PL"),
        (PropertyType::Entity, "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
    ] {
        assert!(
            keys(ptype, raw, &ctx).is_empty(),
            "{ptype} should contribute no blocking key"
        );
    }
}

#[test]
fn every_property_type_derives_keys_without_panicking() {
    let ctx = NormalizeContext::with_region("PL");
    let sample = [
        (PropertyType::Name, "Jan Kowalski"),
        (PropertyType::Email, "jan@example.com"),
        (PropertyType::Phone, "+48601234567"),
        (PropertyType::Url, "https://example.com"),
        (PropertyType::Domain, "example.com"),
        (PropertyType::Ip, "192.0.2.4"),
        (PropertyType::Identifier, "5252248481"),
        (PropertyType::Date, "1978-04-12"),
        (PropertyType::Country, "PL"),
        (PropertyType::Address, "ul. Piękna 12/4, 00-549 Warszawa"),
        (PropertyType::Text, "prose"),
        (PropertyType::Number, "1"),
        (PropertyType::Entity, "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        (
            PropertyType::Checksum,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (PropertyType::Iban, "PL61109010140000071219812874"),
        (
            PropertyType::Wallet,
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
        ),
    ];
    assert_eq!(sample.len(), PropertyType::ALL.len());
    for (ptype, raw) in sample {
        let derived = keys(ptype, raw, &ctx);
        // Keys are sorted, de-duplicated and never empty strings.
        let mut sorted = derived.clone();
        sorted.sort();
        assert_eq!(derived, sorted, "{ptype} keys must come back sorted");
        assert!(derived.iter().all(|k| !k.key.is_empty()), "{ptype}");
    }
}
