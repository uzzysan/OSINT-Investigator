//! The Schema registry: parses YAML, resolves `extends` inheritance,
//! validates everything the ticket asks for, and hands back a catalogue
//! ready to be served.
//!
//! # The custom-schema seam
//!
//! [`SchemaRegistry::from_yaml`] takes its entity and relation sources as
//! plain `(name, yaml)` slices — it does not know or care whether a source
//! came from `include_str!` or a database row. [`SchemaRegistry::builtin`]
//! is just `from_yaml` called with the embedded built-ins. Layering custom
//! Schemas from the `schema_def` table (ticket 15) on top means building one
//! combined slice — the built-in sources followed by the `schema_def` rows,
//! converted to the same `(name, yaml)` shape — and calling `from_yaml`
//! once; a custom Schema is then free to `extends` a built-in one, because
//! resolution does not distinguish where a definition came from. Nothing
//! about that ticket requires changing this one's code, only calling it
//! differently.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::PropertyType;
use crate::schema::caption::validate_template;
use crate::schema::error::{SchemaError, SchemaErrorCode};
use crate::schema::model::{EndpointSpec, EntitySchema, PropertyDef, RelationSchema};
use crate::schema::raw::{RawEntitySchema, RawRelationSchema};

const BUILTIN_ENTITY_SOURCES: &[(&str, &str)] = &[
    (
        "schemas/entities/thing.yaml",
        include_str!("../../schemas/entities/thing.yaml"),
    ),
    (
        "schemas/entities/legal_entity.yaml",
        include_str!("../../schemas/entities/legal_entity.yaml"),
    ),
    (
        "schemas/entities/person.yaml",
        include_str!("../../schemas/entities/person.yaml"),
    ),
    (
        "schemas/entities/company.yaml",
        include_str!("../../schemas/entities/company.yaml"),
    ),
    (
        "schemas/entities/address.yaml",
        include_str!("../../schemas/entities/address.yaml"),
    ),
    (
        "schemas/entities/email.yaml",
        include_str!("../../schemas/entities/email.yaml"),
    ),
    (
        "schemas/entities/phone.yaml",
        include_str!("../../schemas/entities/phone.yaml"),
    ),
    (
        "schemas/entities/website.yaml",
        include_str!("../../schemas/entities/website.yaml"),
    ),
    (
        "schemas/entities/bank_account.yaml",
        include_str!("../../schemas/entities/bank_account.yaml"),
    ),
    (
        "schemas/entities/vehicle.yaml",
        include_str!("../../schemas/entities/vehicle.yaml"),
    ),
    (
        "schemas/entities/document.yaml",
        include_str!("../../schemas/entities/document.yaml"),
    ),
];

const BUILTIN_RELATION_SOURCES: &[(&str, &str)] = &[
    (
        "schemas/relations/ownership.yaml",
        include_str!("../../schemas/relations/ownership.yaml"),
    ),
    (
        "schemas/relations/directorship.yaml",
        include_str!("../../schemas/relations/directorship.yaml"),
    ),
    (
        "schemas/relations/employment.yaml",
        include_str!("../../schemas/relations/employment.yaml"),
    ),
    (
        "schemas/relations/family.yaml",
        include_str!("../../schemas/relations/family.yaml"),
    ),
    (
        "schemas/relations/registration.yaml",
        include_str!("../../schemas/relations/registration.yaml"),
    ),
    (
        "schemas/relations/associate.yaml",
        include_str!("../../schemas/relations/associate.yaml"),
    ),
];

fn known_property_types() -> String {
    PropertyType::ALL
        .iter()
        .map(|t| t.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_property(
    schema_id: &str,
    raw: &crate::schema::raw::RawPropertyDef,
) -> Result<PropertyDef, SchemaError> {
    let ptype = raw.ptype.parse::<PropertyType>().map_err(|_| {
        SchemaError::new(
            schema_id,
            SchemaErrorCode::UnknownPropertyType,
            format!(
                "property `{}` on `{schema_id}` has unknown type `{}`",
                raw.name, raw.ptype
            ),
            format!("use one of: {}", known_property_types()),
        )
    })?;
    Ok(PropertyDef {
        name: raw.name.clone(),
        ptype,
        label: raw.label.clone(),
        multiple: raw.multiple,
        required: raw.required,
    })
}

fn check_no_duplicate_properties(
    schema_id: &str,
    properties: &[PropertyDef],
) -> Result<(), SchemaError> {
    let mut seen = HashSet::new();
    for p in properties {
        if !seen.insert(p.name.as_str()) {
            return Err(SchemaError::new(
                schema_id,
                SchemaErrorCode::DuplicateProperty,
                format!(
                    "property `{}` is declared more than once on `{schema_id}` after inheritance resolves",
                    p.name
                ),
                "rename one of the properties, or remove the duplicate declaration",
            ));
        }
    }
    Ok(())
}

/// Resolve one entity Schema and everything it (transitively) `extends`,
/// memoising into `resolved` and detecting `extends` cycles via `resolving`.
fn resolve_entity(
    id: &str,
    raws: &BTreeMap<String, RawEntitySchema>,
    resolved: &mut BTreeMap<String, EntitySchema>,
    resolving: &mut HashSet<String>,
) -> Result<(), SchemaError> {
    if resolved.contains_key(id) {
        return Ok(());
    }
    if !resolving.insert(id.to_string()) {
        return Err(SchemaError::new(
            id,
            SchemaErrorCode::InheritanceCycle,
            format!("`extends` forms a cycle that includes `{id}`"),
            "remove one of the `extends` edges in the cycle so the ancestry is a DAG",
        ));
    }

    let raw = raws
        .get(id)
        .expect("resolve_entity is only called with ids known to be keys of `raws`");

    let mut ancestor_props: Vec<PropertyDef> = Vec::new();
    let mut ancestors: Vec<String> = Vec::new();
    for parent_id in &raw.extends {
        if !raws.contains_key(parent_id) {
            return Err(SchemaError::new(
                id,
                SchemaErrorCode::UnknownParent,
                format!("`{id}` extends unknown schema `{parent_id}`"),
                format!(
                    "declare an entity schema named `{parent_id}`, or fix the typo in `{id}`'s `extends`"
                ),
            ));
        }
        resolve_entity(parent_id, raws, resolved, resolving)?;
        let parent = resolved
            .get(parent_id)
            .expect("resolve_entity just inserted this id on success");
        ancestor_props.extend(parent.properties.iter().cloned());
        for a in &parent.ancestors {
            if !ancestors.contains(a) {
                ancestors.push(a.clone());
            }
        }
        if !ancestors.contains(parent_id) {
            ancestors.push(parent_id.clone());
        }
    }

    let mut own_props = Vec::with_capacity(raw.properties.len());
    for rp in &raw.properties {
        own_props.push(parse_property(id, rp)?);
    }

    let mut properties = ancestor_props;
    properties.extend(own_props);
    check_no_duplicate_properties(id, &properties)?;

    let known: HashSet<&str> = properties.iter().map(|p| p.name.as_str()).collect();
    match &raw.caption {
        Some(template) => validate_template(id, template, &|name| known.contains(name))?,
        None if !raw.is_abstract => {
            return Err(SchemaError::new(
                id,
                SchemaErrorCode::MissingCaption,
                format!("`{id}` is a concrete schema but declares no caption template"),
                "add a `caption` template that references at least one of this schema's properties",
            ));
        }
        None => {}
    }

    resolving.remove(id);
    resolved.insert(
        id.to_string(),
        EntitySchema {
            id: id.to_string(),
            label: raw.label.clone(),
            plural: raw.plural.clone(),
            extends: raw.extends.clone(),
            ancestors,
            icon: raw.icon.clone(),
            color: raw.color.clone(),
            caption: raw.caption.clone(),
            matchable: raw.matchable,
            is_abstract: raw.is_abstract,
            properties,
        },
    );
    Ok(())
}

fn resolve_all_entities(
    raws: &BTreeMap<String, RawEntitySchema>,
) -> Result<BTreeMap<String, EntitySchema>, SchemaError> {
    let mut resolved = BTreeMap::new();
    for id in raws.keys() {
        let mut resolving = HashSet::new();
        resolve_entity(id, raws, &mut resolved, &mut resolving)?;
    }
    Ok(resolved)
}

fn resolve_relation(
    id: &str,
    raw: &RawRelationSchema,
    entities: &BTreeMap<String, EntitySchema>,
) -> Result<RelationSchema, SchemaError> {
    let mut properties = Vec::with_capacity(raw.properties.len());
    for rp in &raw.properties {
        properties.push(parse_property(id, rp)?);
    }
    check_no_duplicate_properties(id, &properties)?;

    for (end, spec) in [("source", &raw.source), ("target", &raw.target)] {
        for schema_id in &spec.schemas {
            if !entities.contains_key(schema_id) {
                return Err(SchemaError::new(
                    id,
                    SchemaErrorCode::UnknownEndpointSchema,
                    format!("`{id}`'s {end} constraint names unknown entity schema `{schema_id}`"),
                    format!(
                        "declare an entity schema named `{schema_id}`, or remove it from the {end} constraint"
                    ),
                ));
            }
        }
    }

    if let Some(template) = &raw.caption {
        let known: HashSet<&str> = properties.iter().map(|p| p.name.as_str()).collect();
        validate_template(id, template, &|name| known.contains(name))?;
    }

    Ok(RelationSchema {
        id: id.to_string(),
        label: raw.label.clone(),
        source: EndpointSpec {
            label: raw.source.label.clone(),
            schemas: raw.source.schemas.clone(),
        },
        target: EndpointSpec {
            label: raw.target.label.clone(),
            schemas: raw.target.schemas.clone(),
        },
        directed: raw.directed,
        interval: raw.interval,
        color: raw.color.clone(),
        caption: raw.caption.clone(),
        properties,
    })
}

/// The resolved Schema catalogue: every entity and relation Schema, with
/// `extends` inheritance already applied.
///
/// This is the one structure the HTTP and MCP layers will serve once they
/// exist — it derives [`Serialize`] for exactly that reason. Construct it
/// with [`SchemaRegistry::builtin`] for the shipped catalogue, or
/// [`SchemaRegistry::from_yaml`] to resolve an arbitrary set of sources
/// (see the module documentation for how ticket 15 layers custom Schemas on
/// through the same function).
#[derive(Debug, Clone, Serialize)]
pub struct SchemaRegistry {
    entities: BTreeMap<String, EntitySchema>,
    relations: BTreeMap<String, RelationSchema>,
}

impl SchemaRegistry {
    /// Parse, resolve and validate a set of entity and relation Schema
    /// sources. Each source is `(name, yaml)`, where `name` identifies the
    /// source in error messages (a filename for the built-ins; a
    /// `schema_def.id` for a future custom source) — it need not match the
    /// Schema's own `id` field, which is what a parse failure that never
    /// reaches the `id` field is attributed to instead.
    ///
    /// Every validation rule the ticket asks for runs here: an unknown
    /// `PropertyType`, a duplicate Property name after inheritance, a
    /// caption template naming a Property that does not exist, an unknown
    /// parent in `extends`, and a relation endpoint constraint naming a
    /// Schema that does not exist. The first problem found is returned;
    /// nothing here fails partially into an unusable registry.
    pub fn from_yaml(
        entity_sources: &[(&str, &str)],
        relation_sources: &[(&str, &str)],
    ) -> Result<Self, SchemaError> {
        let mut raw_entities: BTreeMap<String, RawEntitySchema> = BTreeMap::new();
        for (name, yaml) in entity_sources {
            let raw: RawEntitySchema = serde_yaml::from_str(yaml).map_err(|e| {
                SchemaError::new(
                    *name,
                    SchemaErrorCode::ParseError,
                    format!("{e}"),
                    "match the entity schema shape: id, label, plural, icon, color, properties[] (each with name, type, label)",
                )
            })?;
            if raw_entities.contains_key(&raw.id) {
                return Err(SchemaError::new(
                    raw.id.clone(),
                    SchemaErrorCode::DuplicateSchemaId,
                    format!(
                        "entity schema `{}` is declared more than once (duplicate found in `{name}`)",
                        raw.id
                    ),
                    "give each entity schema a unique `id`",
                ));
            }
            raw_entities.insert(raw.id.clone(), raw);
        }
        let entities = resolve_all_entities(&raw_entities)?;

        let mut raw_relations: BTreeMap<String, RawRelationSchema> = BTreeMap::new();
        for (name, yaml) in relation_sources {
            let raw: RawRelationSchema = serde_yaml::from_str(yaml).map_err(|e| {
                SchemaError::new(
                    *name,
                    SchemaErrorCode::ParseError,
                    format!("{e}"),
                    "match the relation schema shape: id, label, source, target, directed, interval, color, properties[]",
                )
            })?;
            if raw_relations.contains_key(&raw.id) {
                return Err(SchemaError::new(
                    raw.id.clone(),
                    SchemaErrorCode::DuplicateSchemaId,
                    format!(
                        "relation schema `{}` is declared more than once (duplicate found in `{name}`)",
                        raw.id
                    ),
                    "give each relation schema a unique `id`",
                ));
            }
            raw_relations.insert(raw.id.clone(), raw);
        }
        let mut relations = BTreeMap::new();
        for (id, raw) in &raw_relations {
            relations.insert(id.clone(), resolve_relation(id, raw, &entities)?);
        }

        Ok(Self {
            entities,
            relations,
        })
    }

    /// The built-in catalogue: the provisional starter set from
    /// `04-data-model.md` section 4.1, embedded in the binary at compile
    /// time.
    ///
    /// # Panics
    ///
    /// Panics if a built-in Schema is malformed. That is deliberate: a
    /// broken built-in is a build defect, not a runtime condition, and every
    /// path that reaches this function (startup, and every test that calls
    /// it) is exactly where that should surface loudly rather than as a
    /// silently degraded catalogue.
    pub fn builtin() -> Self {
        Self::from_yaml(BUILTIN_ENTITY_SOURCES, BUILTIN_RELATION_SOURCES).unwrap_or_else(|e| {
            panic!(
                "built-in schema catalogue is malformed — this is a build defect, not a runtime condition.\n  in: {}\n  problem: {}\n  fix: {}",
                e.at, e.message, e.fix
            )
        })
    }

    /// Look up a resolved entity Schema by id.
    pub fn entity(&self, id: &str) -> Option<&EntitySchema> {
        self.entities.get(id)
    }

    /// Look up a resolved relation Schema by id.
    pub fn relation(&self, id: &str) -> Option<&RelationSchema> {
        self.relations.get(id)
    }

    /// Every entity Schema in the catalogue, in id order.
    pub fn entities(&self) -> impl Iterator<Item = &EntitySchema> {
        self.entities.values()
    }

    /// Every relation Schema in the catalogue, in id order.
    pub fn relations(&self) -> impl Iterator<Item = &RelationSchema> {
        self.relations.values()
    }

    /// Look up an entity Schema and refuse the lookup if it is abstract.
    ///
    /// This is the registry's answer to "abstract Schemas cannot be
    /// instantiated": callers that are about to create an entity ask here
    /// first, and get a [`SchemaError`] with a `fix` pointing at a concrete
    /// alternative rather than a schema they cannot actually use.
    pub fn require_instantiable(&self, id: &str) -> Result<&EntitySchema, SchemaError> {
        let schema = self.entities.get(id).ok_or_else(|| {
            SchemaError::new(
                id,
                SchemaErrorCode::UnknownSchema,
                format!("no entity schema named `{id}`"),
                format!(
                    "use one of: {}",
                    self.entities.keys().cloned().collect::<Vec<_>>().join(", ")
                ),
            )
        })?;
        if schema.is_abstract {
            return Err(SchemaError::new(
                id,
                SchemaErrorCode::AbstractInstantiation,
                format!("`{id}` is an abstract schema and cannot be instantiated directly"),
                format!("use a concrete schema that extends `{id}`"),
            ));
        }
        Ok(schema)
    }
}
