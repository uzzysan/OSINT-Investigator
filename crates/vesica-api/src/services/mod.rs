//! The application service layer.
//!
//! Despite living in `vesica-api`, nothing in this module knows what HTTP
//! is. Spec 0001, "The service layer":
//!
//! > A `Services` struct in `vesica-api` — despite the crate name, it is the
//! > application service layer, not the HTTP layer — owns a store handle,
//! > the schema registry and configuration, and exposes one method per use
//! > case taking and returning `vesica-core` types. [...] Both the axum
//! > handlers and the rmcp tools in Spec 0002 will be thin adapters over
//! > this struct, which is what allows one test suite to prove behaviour for
//! > both transports. **No business rule may live in a handler or a tool.**
//!
//! Read that last sentence as a rule about where code goes, because it is
//! the load-bearing structural decision of the whole design. If a rule about
//! what is allowed, what a value means, what gets audited or what an error
//! says is expressible here, it belongs here and nowhere else. A handler
//! parses a request into the types below, calls one method, and serialises
//! what comes back. An MCP tool does the same thing with different
//! plumbing. Neither may decide anything.
//!
//! # Shape of a use case
//!
//! Every mutating method:
//!
//! 1. validates and normalises its input **before** opening a transaction,
//!    so a rejected request never touches the database;
//! 2. opens one transaction via [`vesica_store::Db::transaction`];
//! 3. re-reads what it is about to change inside that transaction, so a
//!    read-modify-write cannot interleave with another caller's;
//! 4. writes the `audit_log` row **first** and the change after it, so that
//!    a failed change cannot leave an orphan audit entry behind — the
//!    ordering is what makes "the audit is in the same transaction" a
//!    testable claim rather than a hopeful one;
//! 5. returns a domain type, never a row and never a DTO.
//!
//! Reads go straight to the reader pool and take no transaction.
//!
//! # The only exceptions to append-only auditing
//!
//! A purge redacts the `before`/`after` payloads of its target's earlier
//! audit entries, in the purge's own transaction, because those payloads are
//! copies of the data the purge was asked to erase
//! (`09-security-and-legal.md`: the audit log retains "only the fact that a
//! deletion occurred"). There are two such paths and there must never be a
//! third: [`vesica_store::repo::audit::redact_case_payloads`] for a Case
//! purge, scoped by `case_id`, and
//! [`vesica_store::repo::audit::redact_target_payloads`] for an Entity purge,
//! scoped to that one row. They are deliberately separate — the same
//! obligation is owed to a purged Entity, but running the case-scoped one for
//! it would erase the payloads of every other Entity in the Case as well.
//! Nothing else in this codebase alters an audit entry, and nothing else may.
//!
//! # Where a fact came from
//!
//! [`source`] owns provenance: Sources, citations and the flagging of facts
//! that have neither. Read its module docs before touching
//! `entity_prop.source_id` — that column is a derived cache of the
//! `citation` table, and exactly one function computes it.

mod case;
mod entity;
mod error;
mod relation;
mod slug;
mod source;
mod tag;

pub use case::{
    AuditAction, AuditPage, AuditRecord, Case, CaseList, CaseQuery, CaseStatus, CaseStatusFilter,
    CreateCase, PurgeCase, REDACTED_BY_PURGE, REDACTED_FIELD, UpdateCase, redaction_marker,
};
pub use entity::{
    Confidence, CreateEntity, DateSpan, EditProperties, Entity, EntityList, EntityQuery,
    EntitySummary, PropertyValue, PropertyWarning, PurgeEntity, UpdateEntity,
};
pub use error::{Actor, ServiceError};
pub use relation::{
    CreateRelation, Endpoint, EndpointRole, EndpointWarning, Interval, Relation, RelationList,
    RelationQuery, RelationSummary, RelationWarningCode, SetInterval, UpdateRelation,
};
pub use source::{
    Citation, CitationTarget, Citations, CiteSource, CiteTarget, Coverage, CreateSource, Source,
    SourceCoverage, SourceKind, SourceList, SourceQuery, SourceUsage, UnsourcedFacts,
    UnsourcedQuery, UnsourcedValue,
};
pub use tag::{TagList, TagSummary};

use std::sync::Arc;

use vesica_core::SchemaRegistry;
use vesica_store::{BoxFuture, Db, SqliteConnection};

/// Rows a list method returns when the caller does not say. Deliberately
/// small: an agent that wants more asks for more and gets a `truncated`
/// flag telling it whether there was more to have.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// The slice of Vesica's configuration the service layer itself needs, as
/// opposed to what the HTTP server needs ([`crate::ApiConfig`]).
///
/// Kept separate so a service can be constructed without inventing a bind
/// address, and so the composition root maps its own `Config` onto exactly
/// the values the business rules read.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Ceiling on how many rows any single service list returns, whatever
    /// the caller asks for. AGENT-BRIEF.md rule 5: every list is bounded.
    pub max_page_size: usize,
    /// ISO-3166-1 alpha-2 region given to a new Case that does not name
    /// one, so that a single-country deployment normalises local phone
    /// numbers correctly without the analyst restating the obvious on every
    /// Case. `None` means new Cases start with no region.
    pub default_region: Option<String>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        ServiceConfig {
            max_page_size: DEFAULT_PAGE_SIZE,
            default_region: None,
        }
    }
}

/// The application service layer: one method per use case, domain types in
/// and out.
///
/// Cheap to clone — [`Db`] is a clone-cheap handle and everything else is
/// behind an `Arc` — so a transport can hold one per connection or one for
/// the process, whichever suits it.
#[derive(Clone)]
pub struct Services {
    db: Db,
    schemas: Arc<SchemaRegistry>,
    config: Arc<ServiceConfig>,
}

impl Services {
    /// Builds the service layer over an open store, a schema registry and
    /// configuration. Three arguments, no I/O, no async: standing one up in
    /// a test has to be as cheap as standing one up in `main`, or the test
    /// suite stops being the place behaviour is proven.
    pub fn new(db: Db, schemas: Arc<SchemaRegistry>, config: ServiceConfig) -> Self {
        Services {
            db,
            schemas,
            config: Arc::new(config),
        }
    }

    /// As [`Services::new`], with the built-in ontology
    /// ([`SchemaRegistry::builtin`]). This is what the composition root
    /// calls until custom schemas layered from `schema_def` exist.
    pub fn with_builtin_schemas(db: Db, config: ServiceConfig) -> Self {
        Services::new(db, Arc::new(SchemaRegistry::builtin()), config)
    }

    /// The ontology every Entity and Relation use case validates against.
    ///
    /// Cases need no Schemas, which is why ticket 0001-08 was not blocked on
    /// the registry — but the registry lives here from the start so that the
    /// next ticket adds methods, not a field.
    pub fn schemas(&self) -> &SchemaRegistry {
        &self.schemas
    }

    /// A handle on the ontology that outlives `&self`.
    ///
    /// A transaction body is `'static` — it is sent to the writer task — so
    /// a use case that needs the registry inside one cannot borrow it from
    /// `self`. Cloning the `Arc` is what makes the ontology available in
    /// there without copying the catalogue.
    pub(crate) fn schemas_handle(&self) -> Arc<SchemaRegistry> {
        Arc::clone(&self.schemas)
    }

    /// The configuration these services were built with.
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    /// The store handle. Exposed for transports that need to check store
    /// health (readiness) — not a licence to write around the service
    /// layer.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Runs one transaction whose body may fail with a [`ServiceError`].
    ///
    /// A thin wrapper over [`Db::transaction`] that pins the error type, so
    /// a use case can abort mid-transaction with `NotFound` or a validation
    /// failure and still get the rollback — and so no call site has to spell
    /// the error type out to help inference along.
    async fn tx<F, T>(&self, f: F) -> Result<T, ServiceError>
    where
        F: for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, Result<T, ServiceError>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        self.db.transaction(f).await
    }

    /// Clamps a caller's requested row budget to
    /// [`ServiceConfig::max_page_size`], defaulting to it when the caller
    /// asks for nothing. A request for zero rows is honoured as one row's
    /// worth of nothing rather than silently becoming "everything".
    fn page_size(&self, requested: Option<usize>) -> usize {
        requested
            .unwrap_or(self.config.max_page_size)
            .min(self.config.max_page_size)
    }
}

/// A three-state change to an optional field, for update requests.
///
/// `Option<T>` cannot express the difference between "leave the description
/// alone" and "clear the description", and a transport that guesses will
/// eventually guess wrong and quietly erase an analyst's notes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Patch<T> {
    /// Leave the stored value as it is.
    #[default]
    Keep,
    /// Replace the stored value.
    Set(T),
    /// Set the stored value to nothing.
    Clear,
}

impl<T> Patch<T> {
    /// Collapses to "no change" (`None`) or "this is the new value"
    /// (`Some(None)` to clear, `Some(Some(v))` to set).
    pub(crate) fn resolve(self) -> Option<Option<T>> {
        match self {
            Patch::Keep => None,
            Patch::Set(v) => Some(Some(v)),
            Patch::Clear => Some(None),
        }
    }
}
