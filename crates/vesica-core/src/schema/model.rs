//! The resolved shapes a [`crate::SchemaRegistry`] hands out: everything an
//! ancestor contributed is already merged in, so a caller never has to walk
//! `extends` itself.

use serde::Serialize;

use crate::PropertyType;

/// One Property declaration, resolved: its type is the real
/// [`PropertyType`] enum, not the string that named it in YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropertyDef {
    /// The Property's name within its Schema. Stable — see
    /// `04-data-model.md` section 4.1.2.
    pub name: String,
    /// What kind of value this Property holds.
    pub ptype: PropertyType,
    /// Human-readable label, for display.
    pub label: String,
    /// Whether the Property can repeat (a person can have five phone
    /// numbers).
    pub multiple: bool,
    /// Whether an instance of the Schema must supply at least one value.
    pub required: bool,
}

/// An entity Schema, fully resolved: its own Properties plus every
/// ancestor's, in ancestor-then-own order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntitySchema {
    /// Stable identifier — see `04-data-model.md` section 4.1.2.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Plural form, for list headings.
    pub plural: String,
    /// The Schema ids named directly in this Schema's own `extends`.
    pub extends: Vec<String>,
    /// The full ancestor chain this Schema resolved to, root-first. Does not
    /// include this Schema's own id.
    pub ancestors: Vec<String>,
    /// Icon name, for the UI.
    pub icon: String,
    /// Accent color, as a `#rrggbb` string.
    pub color: String,
    /// The caption template, e.g. `"{name}"`. `None` only for an abstract
    /// Schema — a concrete Schema is required to declare one. See
    /// [`crate::render_caption`] for how it is rendered, including the rule
    /// for when every Property it names is empty.
    pub caption: Option<String>,
    /// Whether instances of this Schema participate in entity resolution
    /// (cross-case matching).
    pub matchable: bool,
    /// Abstract Schemas exist only for Property inheritance and relation
    /// endpoint constraints; [`crate::SchemaRegistry::require_instantiable`]
    /// refuses to hand one out for instantiation.
    pub is_abstract: bool,
    /// Every Property this Schema carries: its ancestors', in root-first
    /// order, followed by its own.
    pub properties: Vec<PropertyDef>,
}

impl EntitySchema {
    /// Look up one of this Schema's resolved Properties by name.
    pub fn property(&self, name: &str) -> Option<&PropertyDef> {
        self.properties.iter().find(|p| p.name == name)
    }
}

/// A constraint on one end of a relation Schema: which entity Schemas make
/// sense there, and a label for that role.
///
/// `schemas` is advisory, per `04-data-model.md` section 4.1.3: the UI
/// offers only the listed combinations, but nothing here enforces it at
/// write time — investigations routinely meet things the ontology did not
/// anticipate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EndpointSpec {
    /// What to call this end in the UI, e.g. `"Owner"`.
    pub label: String,
    /// The entity Schema ids that make sense here. Every id here is
    /// guaranteed, by registry validation, to name a Schema that exists.
    pub schemas: Vec<String>,
}

/// A relation Schema, fully resolved.
///
/// Relation Schemas do not participate in `extends` inheritance — each
/// declares its own Properties in full, per `04-data-model.md` section
/// 4.1.3.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationSchema {
    /// Stable identifier.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// The constraint on the relation's source end.
    pub source: EndpointSpec,
    /// The constraint on the relation's target end.
    pub target: EndpointSpec,
    /// Whether the relation reads one way (`"owns"`) or is symmetric
    /// (`"is related to"`).
    pub directed: bool,
    /// Whether instances of this relation carry a validity interval
    /// (`date_start` / `date_end` / `date_prec` on the `relation` table).
    pub interval: bool,
    /// Accent color, as a `#rrggbb` string.
    pub color: String,
    /// An optional caption template, rendered the same way as an entity
    /// Schema's. `None` means the relation's caption is just its label.
    pub caption: Option<String>,
    /// This relation's own Properties.
    pub properties: Vec<PropertyDef>,
}

impl RelationSchema {
    /// Look up one of this Schema's Properties by name.
    pub fn property(&self, name: &str) -> Option<&PropertyDef> {
        self.properties.iter().find(|p| p.name == name)
    }
}
