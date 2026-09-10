//! Caption derivation, including the documented rule for what happens when
//! every Property a template names is empty (see
//! `vesica_core::render_entity_caption`'s module documentation).

use std::collections::BTreeMap;

use vesica_core::{
    EntitySchema, PropertyDef, PropertyType, PropertyValues, RelationSchema, SchemaRegistry,
    render_entity_caption, render_relation_caption,
};

fn values(pairs: &[(&str, &[&str])]) -> PropertyValues {
    let mut map: PropertyValues = BTreeMap::new();
    for (name, vs) in pairs {
        map.insert(
            (*name).to_string(),
            vs.iter().map(|v| v.to_string()).collect(),
        );
    }
    map
}

fn schema_with_template(template: &str) -> EntitySchema {
    EntitySchema {
        id: "Test".into(),
        label: "Test thing".into(),
        plural: "Test things".into(),
        extends: vec![],
        ancestors: vec![],
        icon: "circle".into(),
        color: "#000000".into(),
        caption: Some(template.to_string()),
        matchable: true,
        is_abstract: false,
        properties: vec![
            PropertyDef {
                name: "name".into(),
                ptype: PropertyType::Text,
                label: "Name".into(),
                multiple: true,
                required: false,
            },
            PropertyDef {
                name: "role".into(),
                ptype: PropertyType::Text,
                label: "Role".into(),
                multiple: false,
                required: false,
            },
        ],
    }
}

#[test]
fn caption_renders_the_first_non_empty_value_of_a_multiple_property() {
    let schema = schema_with_template("{name}");
    let vals = values(&[("name", &["", "  ", "Jan Kowalski"])]);
    assert_eq!(render_entity_caption(&schema, &vals), "Jan Kowalski");
}

#[test]
fn caption_falls_back_to_the_schema_label_when_the_named_property_is_absent() {
    let schema = schema_with_template("{name}");
    let vals = values(&[]); // no "name" recorded at all
    assert_eq!(render_entity_caption(&schema, &vals), "Test thing");
}

#[test]
fn caption_falls_back_to_the_schema_label_when_every_named_property_is_empty() {
    let schema = schema_with_template("{name}");
    let vals = values(&[("name", &["", "   "])]);
    assert_eq!(render_entity_caption(&schema, &vals), "Test thing");
}

#[test]
fn caption_falls_back_when_a_multi_placeholder_template_is_entirely_empty() {
    let schema = schema_with_template("{name} ({role})");
    let vals = values(&[("name", &[""]), ("role", &["  "])]);
    assert_eq!(
        render_entity_caption(&schema, &vals),
        "Test thing",
        "literal text alone (\" ()\") must not count as a caption"
    );
}

#[test]
fn caption_renders_literal_text_around_a_partially_empty_template() {
    let schema = schema_with_template("{name} ({role})");
    let vals = values(&[("name", &["Jan Kowalski"]), ("role", &[""])]);
    assert_eq!(render_entity_caption(&schema, &vals), "Jan Kowalski ()");
}

#[test]
fn abstract_schema_with_no_caption_template_renders_as_its_label() {
    let mut schema = schema_with_template("{name}");
    schema.caption = None;
    schema.is_abstract = true;
    let vals = values(&[("name", &["Jan Kowalski"])]);
    assert_eq!(render_entity_caption(&schema, &vals), "Test thing");
}

#[test]
fn relation_with_no_caption_template_renders_as_its_label() {
    let registry = SchemaRegistry::builtin();
    let ownership = registry.relation("Ownership").unwrap().clone();
    assert!(ownership.caption.is_none());
    let vals = values(&[("role", &["director"])]);
    assert_eq!(render_relation_caption(&ownership, &vals), "Ownership");
}

#[test]
fn relation_with_a_caption_template_renders_like_an_entity_caption() {
    let registry = SchemaRegistry::builtin();
    let mut ownership: RelationSchema = registry.relation("Ownership").unwrap().clone();
    ownership.caption = Some("{role}".to_string());
    let vals = values(&[("role", &["director"])]);
    assert_eq!(render_relation_caption(&ownership, &vals), "director");
    let empty = values(&[]);
    assert_eq!(render_relation_caption(&ownership, &empty), "Ownership");
}

#[test]
fn built_in_person_caption_uses_the_name_property() {
    let registry = SchemaRegistry::builtin();
    let person = registry.entity("Person").unwrap();
    let vals = values(&[("name", &["Jan Kowalski"])]);
    assert_eq!(render_entity_caption(person, &vals), "Jan Kowalski");

    let empty = values(&[("name", &[""])]);
    assert_eq!(
        render_entity_caption(person, &empty),
        "Person",
        "an empty name must fall back to the schema label, not an empty string"
    );
}
