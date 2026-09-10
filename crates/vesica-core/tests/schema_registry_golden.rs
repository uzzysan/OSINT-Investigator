//! Golden test on the built-in Schema catalogue.
//!
//! Pins the fully resolved form — inheritance applied, `type` strings turned
//! into real [`vesica_core::PropertyType`] variants — of every built-in
//! entity and relation Schema. Schema ids and Property names are a stable
//! contract (`04-data-model.md` section 4.1.2), so an accidental change to
//! the ontology shows up here as a diff instead of silently shipping.
//!
//! To update deliberately (a real ontology change, not an accident):
//! regenerate `tests/golden/schema_registry.json` from
//! `SchemaRegistry::builtin()` serialized with `serde_json::to_string_pretty`
//! and review the diff like any other change to a stable id.

use vesica_core::SchemaRegistry;

#[test]
fn builtin_catalogue_matches_its_golden_file() {
    let registry = SchemaRegistry::builtin();
    let actual = serde_json::to_string_pretty(&registry).expect("registry serializes");
    let expected = include_str!("golden/schema_registry.json");
    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "the resolved built-in schema catalogue no longer matches tests/golden/schema_registry.json.\n\
         If this is a deliberate ontology change, regenerate the golden file and review the diff \
         (schema ids and property names are a stable contract — see 04-data-model.md 4.1.2)."
    );
}

#[test]
fn every_built_in_schema_id_named_in_the_ticket_is_present() {
    let registry = SchemaRegistry::builtin();

    let entity_ids: Vec<&str> = registry.entities().map(|e| e.id.as_str()).collect();
    for expected in [
        "Thing",
        "LegalEntity",
        "Person",
        "Company",
        "Address",
        "Email",
        "Phone",
        "Website",
        "BankAccount",
        "Vehicle",
        "Document",
    ] {
        assert!(
            entity_ids.contains(&expected),
            "expected built-in entity schema `{expected}`, got {entity_ids:?}"
        );
    }
    assert_eq!(
        entity_ids.len(),
        11,
        "unexpected entity schema in catalogue: {entity_ids:?}"
    );

    let relation_ids: Vec<&str> = registry.relations().map(|r| r.id.as_str()).collect();
    for expected in [
        "Ownership",
        "Directorship",
        "Employment",
        "Family",
        "Registration",
        "Associate",
    ] {
        assert!(
            relation_ids.contains(&expected),
            "expected built-in relation schema `{expected}`, got {relation_ids:?}"
        );
    }
    assert_eq!(
        relation_ids.len(),
        6,
        "unexpected relation schema in catalogue: {relation_ids:?}"
    );
}

#[test]
fn only_thing_and_legal_entity_are_abstract() {
    let registry = SchemaRegistry::builtin();
    let abstract_ids: Vec<&str> = registry
        .entities()
        .filter(|e| e.is_abstract)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(
        {
            let mut v = abstract_ids.clone();
            v.sort_unstable();
            v
        },
        vec!["LegalEntity", "Thing"]
    );
}
