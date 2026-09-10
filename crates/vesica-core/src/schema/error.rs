//! Why a Schema definition, or a request against the registry, was refused.

use serde::{Deserialize, Serialize};

/// A stable, machine-readable reason a [`SchemaError`] was raised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SchemaErrorCode {
    /// The YAML text did not parse, or did not match the expected shape.
    ParseError,
    /// Two Schemas of the same kind (entity or relation) declared the same
    /// `id`.
    DuplicateSchemaId,
    /// A Property's `type` string is not a real [`crate::PropertyType`].
    UnknownPropertyType,
    /// After inheritance resolved, the same Property name appeared more than
    /// once.
    DuplicateProperty,
    /// `extends` (or a relation endpoint's `schemas` list) named a Schema id
    /// that does not exist.
    UnknownParent,
    /// `extends` formed a cycle, so no resolution order exists.
    InheritanceCycle,
    /// A caption template could not be parsed (an unterminated `{` or an
    /// empty `{}` placeholder).
    CaptionSyntax,
    /// A caption template named a Property that does not exist on the
    /// Schema (after inheritance, for an entity Schema).
    CaptionUnknownProperty,
    /// A concrete (non-abstract) entity Schema declared no caption template.
    MissingCaption,
    /// A relation endpoint constraint (`source.schemas` / `target.schemas`)
    /// named an entity Schema id that does not exist.
    UnknownEndpointSchema,
    /// A lookup named a Schema id the registry does not have.
    UnknownSchema,
    /// An attempt was made to instantiate an abstract Schema.
    AbstractInstantiation,
}

impl SchemaErrorCode {
    /// The stable string form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParseError => "parse_error",
            Self::DuplicateSchemaId => "duplicate_schema_id",
            Self::UnknownPropertyType => "unknown_property_type",
            Self::DuplicateProperty => "duplicate_property",
            Self::UnknownParent => "unknown_parent",
            Self::InheritanceCycle => "inheritance_cycle",
            Self::CaptionSyntax => "caption_syntax",
            Self::CaptionUnknownProperty => "caption_unknown_property",
            Self::MissingCaption => "missing_caption",
            Self::UnknownEndpointSchema => "unknown_endpoint_schema",
            Self::UnknownSchema => "unknown_schema",
            Self::AbstractInstantiation => "abstract_instantiation",
        }
    }
}

impl std::fmt::Display for SchemaErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A Schema definition was rejected, or a request against a resolved
/// [`crate::SchemaRegistry`] could not be satisfied.
///
/// Carries a `fix` string saying what would make the definition or request
/// valid, in the same spirit as [`crate::ValidationError`] — an agent (or a
/// human wiring up a custom Schema) can act on it without re-reading the
/// source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{at}: {message}")]
pub struct SchemaError {
    /// Where the problem was found: a Schema id when one was parsed
    /// successfully enough to have one, otherwise the source name passed to
    /// [`crate::SchemaRegistry::from_yaml`] (a filename, for the built-ins).
    pub at: String,
    /// A stable, machine-readable reason.
    pub code: SchemaErrorCode,
    /// What was wrong, naming the offending part of the definition.
    pub message: String,
    /// What a valid definition or request looks like.
    pub fix: String,
}

impl SchemaError {
    pub(crate) fn new(
        at: impl Into<String>,
        code: SchemaErrorCode,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            at: at.into(),
            code,
            message: message.into(),
            fix: fix.into(),
        }
    }
}
