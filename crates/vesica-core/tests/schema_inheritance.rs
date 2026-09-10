//! Inheritance resolution: a child Schema carries every ancestor's
//! Properties, in ancestor-then-own order, without having to restate them.

use vesica_core::{PropertyType, SchemaRegistry};

#[test]
fn person_carries_properties_from_both_ancestors() {
    let registry = SchemaRegistry::builtin();
    let person = registry
        .entity("Person")
        .expect("Person is a built-in schema");

    // Declared directly on Person (04-data-model.md 4.1.2).
    let name = person.property("name").expect("Person declares `name`");
    assert_eq!(name.ptype, PropertyType::Name);
    assert!(name.required);
    assert!(name.multiple);

    // Inherited from LegalEntity, one level up.
    let jurisdiction = person
        .property("jurisdiction")
        .expect("Person inherits `jurisdiction` from LegalEntity");
    assert_eq!(jurisdiction.ptype, PropertyType::Country);

    // Inherited from Thing, two levels up — proves multi-level resolution,
    // not just a direct parent.
    let first_seen = person
        .property("firstSeen")
        .expect("Person inherits `firstSeen` from Thing via LegalEntity");
    assert_eq!(first_seen.ptype, PropertyType::Date);
}

#[test]
fn ancestors_are_recorded_root_first_and_extends_stays_the_direct_edge() {
    let registry = SchemaRegistry::builtin();
    let person = registry.entity("Person").unwrap();

    assert_eq!(person.extends, vec!["LegalEntity".to_string()]);
    assert_eq!(
        person.ancestors,
        vec!["Thing".to_string(), "LegalEntity".to_string()],
        "ancestors should list the full chain, root first"
    );
}

#[test]
fn abstract_schemas_carry_inheritance_too() {
    let registry = SchemaRegistry::builtin();
    let legal_entity = registry.entity("LegalEntity").unwrap();
    assert!(legal_entity.is_abstract);
    assert_eq!(legal_entity.ancestors, vec!["Thing".to_string()]);
    assert!(
        legal_entity.property("firstSeen").is_some(),
        "LegalEntity should have inherited Thing's `firstSeen`"
    );
}

#[test]
fn a_schema_with_no_extends_has_no_ancestors() {
    let registry = SchemaRegistry::builtin();
    let thing = registry.entity("Thing").unwrap();
    assert!(thing.extends.is_empty());
    assert!(thing.ancestors.is_empty());
}

#[test]
fn relation_schemas_do_not_inherit_properties() {
    let registry = SchemaRegistry::builtin();
    let ownership = registry.relation("Ownership").unwrap();
    // Ownership has no `extends` concept at all; its properties are exactly
    // its own three (04-data-model.md 4.1.3).
    let names: Vec<&str> = ownership
        .properties
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["percentage", "sharesCount", "role"]);
}
