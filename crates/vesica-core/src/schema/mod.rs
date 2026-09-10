//! The Schema registry: the ontology, as data.
//!
//! Built-in Schemas are YAML under `crates/vesica-core/schemas/`, embedded
//! into the binary with `include_str!` and parsed at startup by
//! [`SchemaRegistry::builtin`]. See `04-data-model.md` section 4.1 for the
//! ontology this implements, and [`registry`]'s module documentation for how
//! ticket 15's custom Schemas are meant to layer on.

mod caption;
mod error;
mod model;
mod raw;
mod registry;

pub use caption::{PropertyValues, render_entity_caption, render_relation_caption};
pub use error::{SchemaError, SchemaErrorCode};
pub use model::{EndpointSpec, EntitySchema, PropertyDef, RelationSchema};
pub use registry::SchemaRegistry;
