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
//! own seeding helpers here — `seed_person`, `seed_small_graph` — beside
//! [`test_services`], rather than re-inventing a fixture per test file.

use std::ops::Deref;
use std::sync::Arc;

use vesica_core::SchemaRegistry;
use vesica_store::testing::{TestStore, test_store};

use crate::services::{ServiceConfig, ServiceError, Services};

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
