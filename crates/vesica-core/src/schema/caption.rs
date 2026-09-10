//! Caption derivation: turning a Schema's template and an instance's
//! Property values into the one-line label a graph, a list or a search
//! result shows.
//!
//! # The all-empty rule
//!
//! A template names one or more Properties by `{propName}`. Rendering
//! substitutes, for each placeholder, the first non-empty value recorded for
//! that Property (`04-data-model.md`'s "first non-empty wins" — relevant
//! when the Property is [`crate::PropertyDef::multiple`] and several values
//! were entered). Literal text around the placeholders passes through
//! unchanged.
//!
//! **When every Property the template names is empty or absent, the caption
//! falls back to the Schema's `label`** (`"Person"`, not an empty string or
//! a string of bare punctuation). This is the rule the ticket asks to be
//! documented: a caption is never blank, and it is never allowed to look
//! like the template leaked into the UI.

use std::collections::BTreeMap;

use crate::schema::error::{SchemaError, SchemaErrorCode};
use crate::schema::model::{EntitySchema, RelationSchema};

/// A Schema instance's Property values, by Property name, in entry order.
/// Mirrors the shape of `entity_prop` rows grouped by `prop`.
pub type PropertyValues = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TemplatePart {
    Literal(String),
    Prop(String),
}

/// Parse a caption template into literal and placeholder parts.
///
/// Errors on an unterminated `{` or an empty `{}` placeholder — both are
/// definitions the Schema author almost certainly did not intend, so they
/// are refused at load time rather than rendered as literal braces forever.
pub(super) fn parse_template(template: &str) -> Result<Vec<TemplatePart>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            let mut closed = false;
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    closed = true;
                    break;
                }
                name.push(c2);
            }
            if !closed {
                return Err(format!(
                    "template `{template}` has an unterminated `{{` with no matching `}}`"
                ));
            }
            if name.is_empty() {
                return Err(format!(
                    "template `{template}` has an empty `{{}}` placeholder"
                ));
            }
            if !literal.is_empty() {
                parts.push(TemplatePart::Literal(std::mem::take(&mut literal)));
            }
            parts.push(TemplatePart::Prop(name));
        } else {
            literal.push(c);
        }
    }
    if !literal.is_empty() {
        parts.push(TemplatePart::Literal(literal));
    }
    Ok(parts)
}

/// Every Property name a template references, in order of first mention.
pub(super) fn template_property_names(parts: &[TemplatePart]) -> Vec<&str> {
    parts
        .iter()
        .filter_map(|p| match p {
            TemplatePart::Prop(name) => Some(name.as_str()),
            TemplatePart::Literal(_) => None,
        })
        .collect()
}

pub(super) fn validate_template(
    source: &str,
    template: &str,
    known: &dyn Fn(&str) -> bool,
) -> Result<(), SchemaError> {
    let parts = parse_template(template)
        .map_err(|message| SchemaError::new(source, SchemaErrorCode::CaptionSyntax, message,
            "write the template as literal text with `{propertyName}` placeholders, each closed with `}`"))?;
    for name in template_property_names(&parts) {
        if !known(name) {
            return Err(SchemaError::new(
                source,
                SchemaErrorCode::CaptionUnknownProperty,
                format!("caption template `{template}` references unknown property `{name}`"),
                "reference only a property declared on this schema or one of its ancestors",
            ));
        }
    }
    Ok(())
}

fn render(caption: Option<&str>, label: &str, values: &PropertyValues) -> String {
    let Some(template) = caption else {
        return label.to_string();
    };
    // Already validated at registry-load time (see `validate_template`), so
    // a parse failure here would mean a Schema reached this point without
    // going through the registry — treat that as a caller error.
    let parts = parse_template(template)
        .expect("caption template must have been validated when the schema was loaded");
    let mut out = String::new();
    let mut any_value = false;
    for part in &parts {
        match part {
            TemplatePart::Literal(s) => out.push_str(s),
            TemplatePart::Prop(name) => {
                if let Some(v) = values
                    .get(name)
                    .and_then(|vs| vs.iter().find(|v| !v.trim().is_empty()))
                {
                    out.push_str(v.trim());
                    any_value = true;
                }
            }
        }
    }
    if any_value && !out.trim().is_empty() {
        out.trim().to_string()
    } else {
        label.to_string()
    }
}

/// Derive an entity instance's caption from its Schema's template and its
/// Property values.
///
/// See the module documentation for the all-empty fallback rule.
pub fn render_entity_caption(schema: &EntitySchema, values: &PropertyValues) -> String {
    render(schema.caption.as_deref(), &schema.label, values)
}

/// Derive a relation instance's caption the same way. A relation Schema
/// with no caption template renders as its label, since not every relation
/// needs one (see `04-data-model.md` section 4.1.3, which shows none for
/// `Ownership`).
pub fn render_relation_caption(schema: &RelationSchema, values: &PropertyValues) -> String {
    render(schema.caption.as_deref(), &schema.label, values)
}
