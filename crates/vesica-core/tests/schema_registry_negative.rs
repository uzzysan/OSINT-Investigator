//! One negative test per registry validation rule.
//!
//! Each builds the smallest YAML that trips exactly one rule and asserts
//! both the [`SchemaErrorCode`] and that the message names what is wrong —
//! not just that construction failed.

use vesica_core::{SchemaError, SchemaErrorCode, SchemaRegistry};

#[track_caller]
fn expect_err(
    entities: &[(&str, &str)],
    relations: &[(&str, &str)],
    code: SchemaErrorCode,
) -> SchemaError {
    match SchemaRegistry::from_yaml(entities, relations) {
        Ok(_) => panic!("expected {code:?} but the registry built successfully"),
        Err(e) => {
            assert_eq!(e.code, code, "wrong error code; message was: {}", e.message);
            assert!(
                !e.fix.is_empty(),
                "every SchemaError must carry a fix string"
            );
            e
        }
    }
}

const THING: (&str, &str) = (
    "thing.yaml",
    "id: Thing\nlabel: Thing\nplural: Things\nabstract: true\nicon: circle\ncolor: \"#000000\"\n",
);

#[test]
fn unknown_property_type_is_rejected() {
    let bad = (
        "bad.yaml",
        "id: Widget\nlabel: Widget\nplural: Widgets\nicon: box\ncolor: \"#111111\"\n\
         caption: \"{sprocket}\"\nproperties:\n  - { name: sprocket, type: sprocketish, label: Sprocket }\n",
    );
    let err = expect_err(&[bad], &[], SchemaErrorCode::UnknownPropertyType);
    assert!(
        err.message.contains("sprocketish"),
        "message should name the bad type: {}",
        err.message
    );
    assert!(
        err.message.contains("sprocket"),
        "message should name the offending property: {}",
        err.message
    );
}

#[test]
fn duplicate_property_after_inheritance_is_rejected() {
    let parent = (
        "parent.yaml",
        "id: Parent\nlabel: Parent\nplural: Parents\nabstract: true\nicon: box\ncolor: \"#111111\"\n\
         properties:\n  - { name: name, type: text, label: Name }\n",
    );
    let child = (
        "child.yaml",
        "id: Child\nlabel: Child\nplural: Children\nextends: [Parent]\nicon: box\ncolor: \"#222222\"\n\
         caption: \"{name}\"\nproperties:\n  - { name: name, type: text, label: Name again }\n",
    );
    let err = expect_err(&[parent, child], &[], SchemaErrorCode::DuplicateProperty);
    assert!(
        err.message.contains("name"),
        "message should name the duplicated property: {}",
        err.message
    );
    assert!(
        err.message.contains("Child"),
        "message should name the schema it was found on: {}",
        err.message
    );
}

#[test]
fn caption_referencing_unknown_property_is_rejected() {
    let bad = (
        "bad.yaml",
        "id: Widget\nlabel: Widget\nplural: Widgets\nicon: box\ncolor: \"#111111\"\n\
         caption: \"{doesNotExist}\"\nproperties:\n  - { name: name, type: text, label: Name }\n",
    );
    let err = expect_err(&[bad], &[], SchemaErrorCode::CaptionUnknownProperty);
    assert!(
        err.message.contains("doesNotExist"),
        "message should name the missing property: {}",
        err.message
    );
}

#[test]
fn caption_with_unterminated_placeholder_is_rejected() {
    let bad = (
        "bad.yaml",
        "id: Widget\nlabel: Widget\nplural: Widgets\nicon: box\ncolor: \"#111111\"\n\
         caption: \"{name\"\nproperties:\n  - { name: name, type: text, label: Name }\n",
    );
    expect_err(&[bad], &[], SchemaErrorCode::CaptionSyntax);
}

#[test]
fn concrete_schema_without_a_caption_is_rejected() {
    let bad = (
        "bad.yaml",
        "id: Widget\nlabel: Widget\nplural: Widgets\nicon: box\ncolor: \"#111111\"\n\
         properties:\n  - { name: name, type: text, label: Name }\n",
    );
    let err = expect_err(&[bad], &[], SchemaErrorCode::MissingCaption);
    assert!(
        err.message.contains("Widget"),
        "message should name the schema missing a caption: {}",
        err.message
    );
}

#[test]
fn unknown_parent_in_extends_is_rejected() {
    let bad = (
        "bad.yaml",
        "id: Widget\nlabel: Widget\nplural: Widgets\nextends: [Nonexistent]\nicon: box\ncolor: \"#111111\"\n\
         caption: \"{name}\"\nproperties:\n  - { name: name, type: text, label: Name }\n",
    );
    let err = expect_err(&[bad], &[], SchemaErrorCode::UnknownParent);
    assert!(
        err.message.contains("Nonexistent"),
        "message should name the missing parent: {}",
        err.message
    );
}

#[test]
fn extends_cycle_is_rejected() {
    let a = (
        "a.yaml",
        "id: A\nlabel: A\nplural: As\nextends: [B]\nabstract: true\nicon: box\ncolor: \"#111111\"\n",
    );
    let b = (
        "b.yaml",
        "id: B\nlabel: B\nplural: Bs\nextends: [A]\nabstract: true\nicon: box\ncolor: \"#222222\"\n",
    );
    expect_err(&[a, b], &[], SchemaErrorCode::InheritanceCycle);
}

#[test]
fn duplicate_entity_schema_id_is_rejected() {
    let one = (
        "one.yaml",
        "id: Widget\nlabel: Widget\nplural: Widgets\nicon: box\ncolor: \"#111111\"\n\
         caption: \"{name}\"\nproperties:\n  - { name: name, type: text, label: Name }\n",
    );
    let two = (
        "two.yaml",
        "id: Widget\nlabel: Widget again\nplural: Widgets\nicon: box\ncolor: \"#222222\"\n\
         caption: \"{name}\"\nproperties:\n  - { name: name, type: text, label: Name }\n",
    );
    let err = expect_err(&[one, two], &[], SchemaErrorCode::DuplicateSchemaId);
    assert!(err.message.contains("Widget"));
}

#[test]
fn relation_endpoint_naming_unknown_schema_is_rejected() {
    let rel = (
        "rel.yaml",
        "id: Owns\nlabel: Owns\nsource: { label: Owner, schemas: [Thing] }\n\
         target: { label: Asset, schemas: [Nonexistent] }\ndirected: true\ninterval: false\n\
         color: \"#333333\"\n",
    );
    let err = expect_err(&[THING], &[rel], SchemaErrorCode::UnknownEndpointSchema);
    assert!(
        err.message.contains("Nonexistent"),
        "message should name the missing endpoint schema: {}",
        err.message
    );
}

#[test]
fn relation_with_unknown_property_type_is_rejected() {
    let rel = (
        "rel.yaml",
        "id: Owns\nlabel: Owns\nsource: { label: Owner, schemas: [Thing] }\n\
         target: { label: Asset, schemas: [Thing] }\ndirected: true\ninterval: false\n\
         color: \"#333333\"\nproperties:\n  - { name: pct, type: percentagey, label: Percent }\n",
    );
    expect_err(&[THING], &[rel], SchemaErrorCode::UnknownPropertyType);
}

#[test]
fn duplicate_relation_property_is_rejected() {
    let rel = (
        "rel.yaml",
        "id: Owns\nlabel: Owns\nsource: { label: Owner, schemas: [Thing] }\n\
         target: { label: Asset, schemas: [Thing] }\ndirected: true\ninterval: false\n\
         color: \"#333333\"\nproperties:\n  - { name: role, type: text, label: Role }\n  - { name: role, type: text, label: Role again }\n",
    );
    expect_err(&[THING], &[rel], SchemaErrorCode::DuplicateProperty);
}

#[test]
fn duplicate_relation_schema_id_is_rejected() {
    let rel1 = (
        "rel1.yaml",
        "id: Owns\nlabel: Owns\nsource: { label: Owner, schemas: [Thing] }\n\
         target: { label: Asset, schemas: [Thing] }\ndirected: true\ninterval: false\ncolor: \"#333333\"\n",
    );
    let rel2 = (
        "rel2.yaml",
        "id: Owns\nlabel: Owns again\nsource: { label: Owner, schemas: [Thing] }\n\
         target: { label: Asset, schemas: [Thing] }\ndirected: true\ninterval: false\ncolor: \"#444444\"\n",
    );
    expect_err(&[THING], &[rel1, rel2], SchemaErrorCode::DuplicateSchemaId);
}

#[test]
fn malformed_yaml_is_rejected_as_parse_error() {
    let bad = ("bad.yaml", "id: [this is not, a valid: schema\n");
    let err = expect_err(&[bad], &[], SchemaErrorCode::ParseError);
    assert_eq!(
        err.at, "bad.yaml",
        "a parse failure is attributed to its source name"
    );
}

#[test]
fn requiring_an_abstract_schema_for_instantiation_is_refused() {
    let registry = SchemaRegistry::builtin();
    let err = registry
        .require_instantiable("Thing")
        .expect_err("Thing is abstract and must be refused");
    assert_eq!(err.code, SchemaErrorCode::AbstractInstantiation);
    assert!(err.message.contains("Thing"));

    let err = registry
        .require_instantiable("LegalEntity")
        .expect_err("LegalEntity is abstract and must be refused");
    assert_eq!(err.code, SchemaErrorCode::AbstractInstantiation);

    registry
        .require_instantiable("Person")
        .expect("Person is concrete and must be instantiable");
}

#[test]
fn requiring_an_unknown_schema_for_instantiation_is_refused() {
    let registry = SchemaRegistry::builtin();
    let err = registry
        .require_instantiable("NoSuchSchema")
        .expect_err("unknown schema must be refused");
    assert_eq!(err.code, SchemaErrorCode::UnknownSchema);
    assert!(err.message.contains("NoSuchSchema"));
}
