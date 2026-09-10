//! The fixture every service-layer test starts from.
//!
//! Spec 0001, Testing Decisions: "Fixture helpers construct a `Services`
//! against a temp database in one call". This is that helper. It layers over
//! [`vesica_store::testing::test_store`], so a test gets a real, freshly
//! migrated SQLite database in its own temporary directory — no mocks, no
//! in-memory fake, nothing that could enforce foreign keys differently from
//! production.
//!
//! ```no_run
//! # async fn example() -> Result<(), vesica_api::services::ServiceError> {
//! use vesica_api::services::{Actor, CreateCase};
//! use vesica_api::testing::test_services;
//!
//! let fx = test_services().await?;
//! let case = fx
//!     .create_case(
//!         CreateCase { name: "Operation Lighthouse".into(), ..Default::default() },
//!         &Actor::analyst(),
//!     )
//!     .await?;
//! assert_eq!(case.slug, "operation-lighthouse");
//! # Ok(())
//! # }
//! // The temp directory is removed when `fx` drops.
//! ```
//!
//! [`TestServices`] dereferences to [`Services`], so a test calls use-case
//! methods straight on the fixture. Reach for [`TestServices::store`] only
//! when a test genuinely needs the store handle itself.
//!
//! This is a normal `pub` module rather than a `#[cfg(test)]` one for the
//! same reason `vesica_store::testing` is: integration tests live in another
//! compilation unit, and so do other crates' tests. Later tickets add their
//! own seeding helpers here — [`seed_person`], [`seed_relation`],
//! [`seed_small_graph`] — beside [`test_services`], rather than
//! re-inventing a fixture per test file.

use std::ops::Deref;
use std::sync::Arc;

use vesica_core::{PropertyValues, SchemaRegistry};
use vesica_store::testing::{TestStore, test_store};

use crate::services::{
    Actor, Case, Citation, CiteSource, CiteTarget, CreateCase, CreateEntity, CreateRelation,
    CreateSource, Entity, Relation, ServiceConfig, ServiceError, Services, SetInterval, Source,
    SourceKind,
};

/// A [`Services`] and the temporary database it runs against. Dropping this
/// removes the database directory; nothing else needs cleaning up.
pub struct TestServices {
    /// The service layer under test.
    pub services: Services,
    /// The store it was built over, for the rare test that needs the raw
    /// handle (proving a rollback, say).
    pub store: TestStore,
}

impl Deref for TestServices {
    type Target = Services;

    fn deref(&self) -> &Services {
        &self.services
    }
}

/// Stands up a working [`Services`] — built-in ontology, default
/// configuration — against a fresh temporary database, in one call.
pub async fn test_services() -> Result<TestServices, ServiceError> {
    test_services_with(ServiceConfig::default()).await
}

/// As [`test_services`], with configuration the test chooses — for
/// exercising a rule that reads it, such as a deployment-wide default
/// region or a small page size.
pub async fn test_services_with(config: ServiceConfig) -> Result<TestServices, ServiceError> {
    let store = test_store().await?;
    let services = Services::new(
        store.db.clone(),
        Arc::new(SchemaRegistry::builtin()),
        config,
    );
    Ok(TestServices { services, store })
}

/// Opens a Case through the service layer, so a test that is really about
/// Entities gets somewhere to put them in one line.
///
/// `region` is the Case's default region — `Some("PL")` for the tests that
/// care how a local phone number normalises, `None` for the ones that do
/// not.
pub async fn seed_case(
    services: &Services,
    name: &str,
    region: Option<&str>,
) -> Result<Case, ServiceError> {
    services
        .create_case(
            CreateCase {
                name: name.to_string(),
                default_region: region.map(str::to_string),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
}

/// Records a `Person` with one name and whatever else is passed, through
/// the service layer, and returns it.
///
/// Takes the properties as `(name, values)` pairs so a test reads as the
/// finding it is describing:
///
/// ```no_run
/// # async fn example(services: &vesica_api::services::Services, case_id: &str)
/// # -> Result<(), vesica_api::services::ServiceError> {
/// use vesica_api::testing::seed_person;
/// let jan = seed_person(services, case_id, &[("name", &["Jan Kowalski"]), ("phone", &["601 234 567"])]).await?;
/// # let _ = jan;
/// # Ok(())
/// # }
/// ```
pub async fn seed_person(
    services: &Services,
    case_id: &str,
    properties: &[(&str, &[&str])],
) -> Result<Entity, ServiceError> {
    services
        .create_entity(
            CreateEntity {
                case_id: case_id.to_string(),
                schema_id: "Person".to_string(),
                properties: property_values(properties),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
}

/// Records an Entity of any Schema, through the service layer.
///
/// [`seed_person`] is the common case; this is for the tests that need the
/// other end of an edge — a `Company` to be a director of, a `Vehicle` to
/// own.
pub async fn seed_entity(
    services: &Services,
    case_id: &str,
    schema_id: &str,
    properties: &[(&str, &[&str])],
) -> Result<Entity, ServiceError> {
    services
        .create_entity(
            CreateEntity {
                case_id: case_id.to_string(),
                schema_id: schema_id.to_string(),
                properties: property_values(properties),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
}

/// Connects two Entities, through the service layer.
///
/// The Relation's Case is taken from the source Entity, because both
/// endpoints have to be in one Case anyway and a test that is really about
/// something else should not have to restate it.
pub async fn seed_relation(
    services: &Services,
    schema_id: &str,
    source: &Entity,
    target: &Entity,
    properties: &[(&str, &[&str])],
) -> Result<Relation, ServiceError> {
    services
        .create_relation(
            CreateRelation {
                case_id: source.case_id.clone(),
                schema_id: schema_id.to_string(),
                source_id: source.id.clone(),
                target_id: target.id.clone(),
                properties: property_values(properties),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
}

/// A Case with a few nodes and both kinds of edge in it, for the tests that
/// are about what happens to a graph rather than about how one was built.
///
/// Spec 0001, Testing Decisions: fixture helpers "seed a Case with a small
/// graph", so that a test reads as the narrative it is describing.
///
/// ```text
///   Jan  ──Directorship (directed, 2015-03 … 2021)──▶  Acme
///    │
///    └────Family (undirected)────  Maria
/// ```
pub struct SmallGraph {
    pub case: Case,
    /// A `Person`, the busy one: director of [`SmallGraph::acme`] and
    /// family of [`SmallGraph::maria`].
    pub jan: Entity,
    /// A `Person`, related to Jan by an undirected edge.
    pub maria: Entity,
    /// A `Company`.
    pub acme: Entity,
    /// Jan's directorship of Acme: directed, with a validity interval.
    pub directorship: Relation,
    /// Jan and Maria: undirected, so no arrowhead is asserted.
    pub family: Relation,
}

/// Builds [`SmallGraph`] in one call.
pub async fn seed_small_graph(services: &Services) -> Result<SmallGraph, ServiceError> {
    let case = seed_case(services, "Operation Lighthouse", Some("PL")).await?;
    let jan = seed_person(services, &case.id, &[("name", &["Jan Kowalski"])]).await?;
    let maria = seed_person(services, &case.id, &[("name", &["Maria Kowalska"])]).await?;
    let acme = seed_entity(
        services,
        &case.id,
        "Company",
        &[("name", &["Acme sp. z o.o."])],
    )
    .await?;

    let directorship = services
        .create_relation(
            CreateRelation {
                case_id: case.id.clone(),
                schema_id: "Directorship".to_string(),
                source_id: jan.id.clone(),
                target_id: acme.id.clone(),
                properties: property_values(&[("role", &["chair"])]),
                interval: Some(SetInterval {
                    start: Some("2015-03".to_string()),
                    end: Some("2021".to_string()),
                    precision: None,
                }),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await?;
    let family = seed_relation(services, "Family", &jan, &maria, &[]).await?;

    Ok(SmallGraph {
        case,
        jan,
        maria,
        acme,
        directorship,
        family,
    })
}

/// Records a `url` Source in one Case, through the service layer, so a test
/// that is really about provenance gets something to cite in one line.
///
/// The title is derived from the URL because a test that has to invent one
/// is a test saying something it does not mean.
pub async fn seed_source(
    services: &Services,
    case_id: &str,
    url: &str,
) -> Result<Source, ServiceError> {
    services
        .create_source(
            CreateSource {
                case_id: case_id.to_string(),
                kind: Some(SourceKind::Link),
                title: Some(format!("Filing at {url}")),
                url: Some(url.to_string()),
                retrieved_at: Some("2024-05-01T09:00:00Z".to_string()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
}

/// Cites a Source for a whole Entity.
pub async fn cite_entity(
    services: &Services,
    source: &Source,
    entity: &Entity,
) -> Result<Citation, ServiceError> {
    services
        .cite_source(
            CiteSource {
                source_id: source.id.clone(),
                target: CiteTarget::Entity {
                    entity_id: entity.id.clone(),
                },
                note: None,
            },
            &Actor::analyst(),
        )
        .await
}

/// Cites a Source for one value of one of an Entity's Properties, named the
/// way a read reports it: the Property and the value's position within it.
pub async fn cite_property(
    services: &Services,
    source: &Source,
    entity: &Entity,
    prop: &str,
    seq: usize,
) -> Result<Citation, ServiceError> {
    services
        .cite_source(
            CiteSource {
                source_id: source.id.clone(),
                target: CiteTarget::EntityProperty {
                    entity_id: entity.id.clone(),
                    prop: prop.to_string(),
                    seq: Some(seq),
                },
                note: None,
            },
            &Actor::analyst(),
        )
        .await
}

/// Turns the `(name, values)` pairs a test writes into the map the service
/// layer takes.
pub fn property_values(pairs: &[(&str, &[&str])]) -> PropertyValues {
    pairs
        .iter()
        .map(|(prop, values)| {
            (
                (*prop).to_string(),
                values.iter().map(|v| (*v).to_string()).collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_fixture_is_usable_and_isolated() {
        let a = test_services().await.unwrap();
        let b = test_services().await.unwrap();
        assert_ne!(a.store.dir_path(), b.store.dir_path());

        // The registry is wired in even though Cases do not need it, so the
        // next ticket adds methods rather than a field.
        assert!(a.schemas().entities().count() > 0);
        assert_eq!(
            a.config().max_page_size,
            ServiceConfig::default().max_page_size
        );
    }
}
