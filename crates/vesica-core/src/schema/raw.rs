//! The YAML shapes as written by hand, before inheritance is resolved or
//! `type` strings are checked against [`crate::PropertyType`]. Private to
//! the crate: nothing outside `schema::registry` should see an unresolved
//! Schema.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct RawPropertyDef {
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) ptype: String,
    pub(super) label: String,
    #[serde(default)]
    pub(super) multiple: bool,
    #[serde(default)]
    pub(super) required: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawEntitySchema {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) plural: String,
    #[serde(default)]
    pub(super) extends: Vec<String>,
    #[serde(rename = "abstract", default)]
    pub(super) is_abstract: bool,
    pub(super) icon: String,
    pub(super) color: String,
    #[serde(default)]
    pub(super) caption: Option<String>,
    #[serde(default)]
    pub(super) matchable: bool,
    #[serde(default)]
    pub(super) properties: Vec<RawPropertyDef>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawEndpointSpec {
    pub(super) label: String,
    pub(super) schemas: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawRelationSchema {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) source: RawEndpointSpec,
    pub(super) target: RawEndpointSpec,
    pub(super) directed: bool,
    pub(super) interval: bool,
    pub(super) color: String,
    #[serde(default)]
    pub(super) caption: Option<String>,
    #[serde(default)]
    pub(super) properties: Vec<RawPropertyDef>,
}
