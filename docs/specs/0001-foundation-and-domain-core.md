---
id: 0001
title: Foundation and domain core (M0 + M1)
labels: [ready-for-agent]
status: ready
milestones: [M0, M1]
depends_on:
  - docs/wayfinder/tickets/002-build-and-test-loop.md
  - docs/wayfinder/tickets/003-product-name.md
  - docs/wayfinder/tickets/004-schema-catalogue.md
created: 2026-09-08
---

# Spec 0001 — Foundation and domain core

Covers milestones **M0** and **M1** of [`PLAN.md`](../../PLAN.md). Ends with a
service layer that can hold a real investigation correctly, with no HTTP, no MCP
and no UI. Spec 0002 will put transports on top of it.

---

## Problem Statement

An investigator's findings currently live in notes, spreadsheets and browser
tabs. A phone number written down in March and a company registration found in
September are related, but nothing holds that relationship, so the connection
exists only in the investigator's head and evaporates when the case is set down
for a fortnight.

The tools that solve this are unsatisfactory in specific ways: Maltego is
commercial, desktop-bound and offers nothing an AI agent can drive; Aleph
demands Elasticsearch, Postgres, Redis and a worker fleet before it stores a
single name; free-form note tools impose no types, so nothing can be queried,
compared or deduplicated.

Before any of that can be addressed, something has to hold the data correctly.
A **Case** that loses a source, a **Property** that silently discards the raw
value the investigator typed, a **Relation** that survives the deletion of its
endpoint, or an audit trail with a hole in it — each of these quietly corrupts an
investigation, and the corruption is discovered months later when a conclusion is
challenged and the evidence cannot be reconstructed.

There is also a prior problem. The repository currently contains documentation
and nothing else. There is no way to build it, no way to test it, and no way to
put it on a machine. Until that exists, every subsequent milestone is blocked.

## Solution

Two things, in order.

**A repository that builds, tests, lints and deploys an application that does
nothing.** A Cargo workspace with the crate boundaries from
[`docs/03-architecture.md`](../03-architecture.md#33-cargo-workspace), a binary
that loads configuration and serves a health endpoint, a web scaffold that
compiles into that binary, CI that enforces formatting, lints, tests, dependency
advisories and secret scanning, and an installer that carries all of it onto a
clean machine and leaves a running service behind. Nothing here is a feature.
Everything after it depends on it existing.

**A domain core that holds an investigation correctly.** The ontology — Schemas,
Properties and their Property types — loaded as data rather than compiled in. A
normalisation layer that turns `0601 234 567` into `+48601234567` and `12/04/1978`
into a partial date with stated precision, while never discarding what the
investigator actually typed. Storage in SQLite that survives a kill -9 mid-write.
Bounded graph traversal that returns a partial answer and says so, rather than
exhausting memory on a hairball. An append-only audit log that records which
**Actor** — the analyst or a named agent — asserted each fact, and when.

All of it reachable through a single service layer, which is also where the HTTP
API and the MCP server will attach in the next spec. The layer is the product at
the end of this spec: no transport, no interface, just a correct core with tests
that prove it.

## User Stories

### Building and deploying (M0)

1. As a maintainer, I want a Cargo workspace with the eight crates already
   separated, so that the dependency direction is enforced by the compiler
   from the first commit rather than agreed and then eroded.
2. As a maintainer, I want `vesica-core` to declare no internal dependencies and
   perform no I/O, so that domain rules can be tested without a database and
   stay testable as the project grows.
3. As a maintainer, I want a single `just` recipe for each of dev, build, test,
   lint, fmt and audit, so that neither I nor an implementing agent has to
   remember an incantation.
4. As a maintainer, I want CI to fail on a formatting drift, a clippy warning, a
   failing test, a vulnerable dependency or a leaked secret, so that these are
   caught at push time rather than at review time.
5. As a maintainer, I want the web scaffold to compile into the binary, so that
   there is never a static-file directory that disagrees with the build.
6. As a maintainer, I want a development mode where the frontend hot-reloads
   against the running Rust server, so that UI work in M3 is not gated on a
   full rebuild per change.
7. As an operator, I want to clone the repository and run one script, so that I
   end up with a running service without reading documentation first.
8. As an operator, I want the installer to tell me what it is about to do and
   what it did, so that I can tell whether it succeeded without inspecting the
   system afterwards.
9. As an operator, I want the installer to pick a free port when my preferred
   one is taken, so that a collision does not become a debugging session.
10. As an operator, I want the installer to refuse to overwrite an existing
    `.env`, so that re-running it cannot destroy my configuration.
11. As an operator, I want configuration to come entirely from environment
    variables and `.env`, so that nothing sensitive is ever committed.
12. As an operator, I want a health endpoint and a readiness endpoint that mean
    different things, so that a supervisor can distinguish "alive" from "able to
    serve".
13. As an operator, I want structured logs on stdout at a configurable level, so
    that journald captures them and I can turn up detail without a rebuild.
14. As a maintainer, I want migrations applied automatically at startup inside a
    transaction, so that a half-applied schema is not a state the application
    can reach.

### Cases (M1)

15. As an analyst, I want to create a Case with a name and a description, so that
    an investigation has a container with a stated purpose.
16. As an analyst, I want each Case to have a slug, so that it can be referenced
    stably in URLs and by agents.
17. As an analyst, I want to set a default region on a Case, so that local phone
    numbers entered without a country code normalise correctly.
18. As an analyst, I want to archive a Case rather than delete it, so that
    concluded work stops cluttering my list without being destroyed.
19. As an analyst, I want deleting a Case to be reversible for a period, so that
    a misclick is not a catastrophe.
20. As an analyst, I want a separate, explicitly confirmed purge action, so that
    when I do need data gone, it is actually gone.
21. As an analyst, I want every Entity, Relation and Source to belong to exactly
    one Case, so that the boundary between investigations is unambiguous.

### Schemas and the ontology

22. As an analyst, I want built-in Schemas for the things I actually record —
    people, companies, addresses, accounts, vehicles, websites, documents — so
    that I am not inventing a data model while trying to work.
23. As an analyst, I want each Schema to declare its Properties with types, so
    that the system knows a phone number is a phone number.
24. As an analyst, I want Properties to repeat, so that a person can have five
    phone numbers without me choosing which one is real.
25. As an analyst, I want Schema inheritance, so that everything that is a
    `LegalEntity` shares the properties that all legal entities have.
26. As a deployment owner, I want to define a custom Schema without forking the
    project, so that a domain the built-ins do not cover is not a dead end.
27. As a deployment owner, I want to add Properties to a built-in Schema, so that
    I can extend rather than replace.
28. As an agent, I want to read the full Schema catalogue including custom
    additions, so that I know what I am allowed to write before I write it.
29. As a maintainer, I want Schema ids and Property names treated as a stable
    contract, so that a rename is a deliberate migration rather than an
    accidental break.

### Properties, values and normalisation

30. As an analyst, I want the exact value I typed preserved forever, so that I
    can see the source's own wording rather than the system's interpretation
    of it.
31. As an analyst, I want a normalised form stored alongside it, so that the same
    fact recorded two different ways can be recognised as the same fact.
32. As an analyst, I want phone numbers normalised to E.164 using the Case's
    region, so that `0601 234 567` and `+48 601 234 567` are one number.
33. As an analyst, I want email addresses folded consistently, so that
    `J.Kowalski@Example.COM` matches `jkowalski@example.com`.
34. As an analyst, I want identifiers with checksums — IBAN, VIN, NIP, PESEL —
    validated on entry, so that a transposed digit is caught while I still
    remember where I read it.
35. As an analyst, I want a failed checksum to warn rather than refuse, so that a
    genuinely malformed identifier found in a genuine document can still be
    recorded, flagged.
36. As an analyst, I want partial dates, so that "sometime in 2019" is recorded
    as 2019 and not silently as 1 January 2019.
37. As an analyst, I want each date's precision stored explicitly, so that the
    timeline can later draw a year-wide band rather than a false point.
38. As an analyst, I want names folded for comparison without losing diacritics
    in the stored value, so that `Łukasz` displays correctly and still matches
    `Lukasz`.
39. As an analyst, I want URLs normalised with tracking parameters stripped, so
    that the same page cited twice is recognised as one source.
40. As a maintainer, I want normalisation to live in exactly one place, so that
    the write path and the resolver can never disagree about what two values mean.
41. As a maintainer, I want normalised values recomputable from raw values, so
    that improving a normalisation rule is a rescan rather than a data loss event.

### Entities

42. As an analyst, I want to create an Entity by choosing a Schema and filling
    Properties, so that recording a finding takes seconds.
43. As an analyst, I want a caption derived automatically from the Entity's
    Properties, so that I do not have to name every node by hand.
44. As an analyst, I want to override that caption, so that a node can be labelled
    the way I think about it rather than the way its data reads.
45. As an analyst, I want a confidence flag on every Entity — confirmed, probable,
    hypothesis — so that a guess is never mistaken for a fact by me or by anyone
    reading my report.
46. As an analyst, I want to tag Entities freely, so that I can group things the
    ontology has no opinion about.
47. As an analyst, I want deleted Entities restorable for thirty days, so that
    deleting is not a decision I have to be certain about.
48. As an analyst, I want an Entity that was merged away to keep resolving by its
    original id, so that a link written in a report six months ago still works.
49. As an agent, I want validation errors that state what was wrong and what to
    send instead, so that I can correct myself without asking a human.
50. As an agent, I want to generate an Entity id myself before writing, so that I
    can build a whole subgraph locally and submit it in one piece.

### Relations

51. As an analyst, I want typed, directed Relations between Entities, so that
    "owns" and "is owned by" are the same edge read from two ends.
52. As an analyst, I want Relations to carry their own Properties, so that a
    shareholding can record its percentage.
53. As an analyst, I want Relations to carry a validity Interval, so that a
    directorship that ended in 2021 is not shown as current.
54. As an analyst, I want the system to know which Schema combinations are
    sensible, so that I am guided toward correct modelling.
55. As an analyst, I want to override that guidance with a warning, so that
    reality is not constrained by the ontology's imagination.
56. As an analyst, I want deleting an Entity to take its Relations with it, so
    that the graph never contains an edge to nothing.
57. As an analyst, I want undirected Relations for genuinely symmetric
    associations, so that an arrowhead does not assert a direction the evidence
    does not support.

### Sources and provenance

58. As an analyst, I want to attach a Source to an Entity, a Relation or an
    individual Property value, so that provenance is recorded at the granularity
    the claim was made at.
59. As an analyst, I want each Source to record when it was retrieved, so that a
    fact can be dated to the moment it was observed rather than the moment it
    was typed.
60. As an analyst, I want facts without a Source permitted but flagged, so that
    capture stays fast and the gaps stay visible.
61. As an analyst, I want to know which of my facts are unsourced, so that I can
    close those gaps before anyone challenges them.

### Search and traversal

62. As an analyst, I want full-text search across captions and Property values,
    so that I can find a half-remembered name.
63. As an analyst, I want search to ignore diacritics, so that searching
    `Kowalski` finds `Kowalskí`.
64. As an analyst, I want to search across every Case at once, so that I find out
    I have seen a name before.
65. As an analyst, I want to fetch an Entity's neighbours to a stated depth, so
    that I can explore outward from something I know.
66. As an analyst, I want traversal bounded by depth and by a node budget, so that
    expanding a hub does not hang the application.
67. As an analyst, I want a truncated result labelled as truncated, so that I
    never mistake a partial answer for a complete one.
68. As an analyst, I want the shortest path between two Entities, so that I can
    answer "how are these two connected?".
69. As an analyst, I want connected components, so that I can see which parts of a
    Case are actually joined and which are still isolated.

### Audit and integrity

70. As an analyst, I want every change recorded with its Actor, so that I can tell
    what I asserted from what an agent asserted.
71. As an analyst, I want to see an Entity's full change history, so that I can
    reconstruct how a conclusion was reached.
72. As an analyst, I want related changes grouped under one batch, so that an
    import or a merge can be understood, and later undone, as one act.
73. As an analyst, I want the audit log append-only, so that history cannot be
    quietly rewritten — including by me.
74. As an operator, I want an integrity check command, so that I can verify the
    database is sound without interpreting SQL myself.
75. As an operator, I want a multi-step write to either complete or not happen,
    so that a crash mid-write leaves a consistent database rather than a puzzle.

## Implementation Decisions

### Workspace and crate boundaries

Eight crates as set out in
[`docs/03-architecture.md`](../03-architecture.md#33-cargo-workspace). This spec
brings `vesica-core`, `vesica-store` and `vesica` to a working state, creates
`vesica-api` with only health and readiness, and creates the remaining crates as
compiling stubs so the dependency graph is established and enforced from the
start.

The one-way dependency rule is enforced mechanically, not by convention: CI fails
if `vesica-core` gains an internal dependency or an I/O crate. `vesica-core`
carries `#![forbid(unsafe_code)]`, as do all crates unless a documented exception
exists.

### The service layer

A `Services` struct in `vesica-api` — despite the crate name, it is the
application service layer, not the HTTP layer — owns a store handle, the schema
registry and configuration, and exposes one method per use case taking and
returning `vesica-core` types. It performs authorisation-free business
orchestration: validate, normalise, persist, audit, publish.

This is deliberate and load-bearing. Both the axum handlers and the rmcp tools in
Spec 0002 will be thin adapters over this struct, which is what allows one test
suite to prove behaviour for both transports. No business rule may live in a
handler or a tool.

The struct is constructed from configuration and a store handle, so a test
constructs it against a temporary database in a few lines.

### Ontology as data

Built-in Schemas are YAML files under `crates/vesica-core/schemas/`, embedded into
the binary at compile time. The registry parses them at startup, resolves
`extends` inheritance, validates internal consistency (no unknown Property types,
no duplicate Property names after inheritance, caption templates referencing
Properties that exist) and fails startup loudly if a built-in Schema is
malformed — a broken built-in is a build defect, not a runtime condition.

Custom Schemas from the `schema_def` table are layered over the built-ins after
they load. A custom Schema may extend a built-in and may add Properties to one.
A custom definition that conflicts with a built-in is rejected at write time with
an explanatory error, not silently at startup.

**Open dependency.** The exact catalogue — which Schemas, which Properties, which
match weights — is
[ticket 004](../wayfinder/tickets/004-schema-catalogue.md) and is not settled.
This spec therefore covers the *registry mechanism* in full and a **starter set**
sufficient to exercise it: `LegalEntity` (abstract), `Person`, `Company`,
`Address`, `Email`, `Phone`, `Website`, `BankAccount`, `Vehicle`, `Document`,
plus relation Schemas `Ownership`, `Directorship`, `Employment`, `Family`,
`Registration` and `Associate`. The implementing agent should treat this set as
provisional and expect it to be replaced wholesale when 004 closes — which is
cheap precisely because the catalogue is data.

### Property types and normalisation

`PropertyType` is an enum in `vesica-core`. Each variant owns three pure
functions: parse-and-validate, normalise, and blocking-key derivation. Blocking
keys are unused in this spec but defined here so the resolver in M5 has nothing
to invent.

Normalisation returns a result carrying the normalised value plus zero or more
warnings. A checksum failure is a **warning**, not an error: the value is stored,
flagged, and surfaced. This is a deliberate reversal of the usual default —
investigative data is frequently wrong in the source, and refusing to record what
a document actually says is worse than recording it with a flag.

Partial dates are their own type, not a string. They carry a lower bound, an
upper bound and a precision (`year`, `month`, `day`, `instant`). All date
comparison goes through them. There is no code path in which a partial date is
widened to a point.

Crate choices are fixed here so they are not relitigated per pull request:
`strsim`, `rphonetic`, `unicode-normalization`, `deunicode`, `phonenumber`,
`iban_validate`, `ulid`. Polish identifier checksums (NIP, PESEL, REGON) and VIN
are hand-rolled; they are short, well-specified, and not worth a dependency.

### Storage

Schema exactly as specified in [`docs/04-data-model.md`](../04-data-model.md),
implemented as numbered `sqlx` migrations applied at startup inside a
transaction. Compile-time-checked queries throughout; no SQL built by string
concatenation anywhere, including in dynamic filters, which use bound parameter
lists.

Pragmas set on every connection: `journal_mode=WAL`, `foreign_keys=ON`,
`synchronous=NORMAL`, `busy_timeout`.

**Concurrency**: one writer task fed by a channel, plus a reader pool. This makes
lock contention structurally impossible rather than merely unlikely, and removes
the need for optimistic retry logic. Every multi-step write is one transaction.

**Repositories** (`CaseRepo`, `EntityRepo`, `RelationRepo`, `SourceRepo`,
`AuditRepo`) are traits with a SQLite implementation. The trait layer exists as
the seam for a future Postgres backend
([ADR-0001](../adr/0001-single-binary-sqlite.md)) and is deliberately unused by a
second implementation. It is not a test seam — see Testing Decisions.

**Traversal** is recursive CTEs, always bounded by both a depth limit and a node
budget, always returning a `truncated` flag. There is no unbounded traversal API,
not even an internal one, because an internal one becomes an external one.

**FTS5** with `unicode61 remove_diacritics 2`, kept in sync by triggers on
`entity` and `entity_prop` rather than by application code, so a write path that
forgets to reindex cannot exist.

### Soft deletion

`deleted_at` on Cases, Entities and Relations. Every read filters it. Restore
clears it. Purge is a separate operation that deletes rows and is refused unless
explicitly confirmed by the caller.

Deleting an Entity cascades to its Relations. Merging sets `merged_into` and
leaves the row addressable, so an id quoted in an old report or held by an agent
still resolves — to the surviving Entity, with an indication that it was merged.

### Audit

Every mutating service method writes an `audit_log` row in the same transaction
as the change. Actor is a string supplied by the caller, defaulting to `human`.
Related changes share a `batch_id`. The table has no update or delete path in the
code at all; append-only is enforced by the absence of the capability, not by a
convention.

### Configuration

`figment` or equivalent layering environment variables over `.env` over defaults.
Every key in `.env.example` is represented. The config struct validates on load
and fails startup with a message naming the offending key. Secrets are never
logged, including truncated.

### Deployment (M0 portion)

`deploy/install.sh` implements preflight, toolchain detection, build, free-port
scan, `.env` generation, data directory creation under a dedicated system user,
migration, systemd unit installation and a health-check verification loop, per
[`docs/08-deployment.md`](../08-deployment.md). At the end of this spec it
installs an application that answers `/api/v1/health` and `/api/v1/ready` and
nothing else — which is the point: the deployment path is proven before there is
anything to deploy.

### Naming

The workspace uses `vesica` and `osint-*` as specified.
[Ticket 003](../wayfinder/tickets/003-product-name.md) is open, and these names
reach crate names, the systemd unit, the install path and the default data
directory. The implementing agent should keep the name in exactly the places
`docs/08-deployment.md` lists and avoid spreading it into strings, so that a
rename remains a contained change.

## Testing Decisions

### What makes a good test here

A test asserts what a caller observes, never how it was achieved. It goes through
the service layer or through a pure function's signature, and it never reaches
into a repository, inspects a private field, or asserts on the shape of a SQL
statement. The question a test answers is "if I do this, what do I get back and
what is true afterwards" — where "what is true afterwards" is itself established
by a subsequent service call, not by a raw query.

The practical test: a refactor that changes no behaviour should break no test. If
splitting a function in two breaks the suite, the suite was measuring the wrong
thing.

### Seam 1 — the service layer (primary)

Nearly all behaviour is tested here, against a **real SQLite database in a
temporary directory**, created fresh per test and discarded after. No mocking of
the store, no in-memory fake. The store is fast enough that mocking it would buy
nothing and cost fidelity — a fake that does not enforce foreign keys is a fake
that hides the bugs worth catching.

Fixture helpers construct a `Services` against a temp database in one call, and
seed a Case with a small graph. Tests read as narratives: create a Case, add a
Person with a phone, add a Company, relate them, assert the neighbour query
returns what it should.

Covered at this seam: Case lifecycle including archive, soft delete, restore and
purge; Entity and Relation CRUD; Property multiplicity; caption derivation and
override; confidence and tags; Source attachment and unsourced-fact flagging;
search including diacritic folding and cross-Case results; traversal including
depth limits, node budgets and the truncation flag; shortest path; connected
components; audit completeness and batch grouping; cascade behaviour; transaction
atomicity under an induced mid-write failure.

This is the seam that pays for itself twice: Spec 0002's HTTP handlers and MCP
tools are adapters over the same struct, so this suite already proves their
behaviour, and the transport tests reduce to wiring checks.

### Seam 2 — `vesica-core` pure functions (secondary)

Normalisation, validation, Schema registry resolution and caption derivation are
pure functions with no I/O, and their edge cases are numerous, cheap to enumerate
and genuinely tricky. Testing them through the service layer would be verbose
without being more truthful.

- **Table-driven tests** per Property type: input, expected normalised output,
  expected warnings. Phone numbers across regions; emails with tags and
  punctuation; IBAN, VIN, NIP, PESEL, REGON valid and corrupted; URLs with
  tracking parameters; names with diacritics, transliterations and ordering
  differences; partial dates at each precision, including the ones that trip
  people up (`2019-02-29`, `2019-13`, two-digit years).
- **Property-based tests** (`proptest`) for the invariants that must hold for
  every input: normalisation is idempotent; it never panics on arbitrary Unicode;
  raw values are recoverable unchanged; a partial date's lower bound never
  exceeds its upper bound.
- **Golden tests** on the Schema registry: every built-in Schema loads, inherits
  correctly, and produces a stable resolved form. The golden file makes an
  accidental ontology change visible in review, which matters because Schema ids
  are a public contract.

### What is deliberately not a seam

**Repositories are not tested directly.** SQL correctness, FTS trigger sync and
recursive-CTE traversal are all reachable and asserted through the service layer.
A parallel repository suite would duplicate that coverage while coupling tests to
an implementation detail that ADR-0001 explicitly wants free to change.

**The HTTP layer is not tested for behaviour in this spec**, because it has none
yet — only health and readiness, which get a smoke test confirming they return
the right status and that readiness fails when the data directory is not
writable.

### Prior art

There is none in this repository; the codebase is documentation only. These
conventions are therefore established here and will be cited by later specs:
`tests/` per crate for integration tests, `#[cfg(test)]` modules for pure-function
tests, a shared `vesica-testkit` dev-dependency for fixtures once a second crate
needs them, and `insta` for golden assertions.

### CI

`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`,
`cargo deny check`, `gitleaks detect`, and the web build. A clean-box install
test running `deploy/install.sh` inside a Debian container and asserting a
healthy service is part of M7, not this spec — but the installer is written here
so that test has something to run.

## Out of Scope

- **The HTTP API beyond health and readiness**, and **the MCP server entirely**.
  Both are Spec 0002. This spec ends at the service layer on purpose.
- **Entity resolution.** Blocking-key derivation is *defined* per Property type
  because that is where the knowledge lives, but no keys are computed, no
  candidates generated, no `match_job` queue drained. M5.
- **Attachments and blob storage.** Sources are recorded; files are not uploaded,
  hashed or stored. M4.
- **The timeline.** Partial dates and precision are implemented because they are
  a domain concern; `timeline_event` materialisation and any timeline query are
  M4.
- **The web UI.** M0 creates the scaffold and proves it embeds. It renders
  nothing beyond a placeholder. M3.
- **Import and export** in every form. M6.
- **Authentication**, including token mode. Solo mode means no check to write
  yet; the middleware seam appears with the HTTP surface in Spec 0002.
- **Merge and undo.** `merged_into` exists in the schema and is honoured on read;
  the merge operation itself is M5.
- **Backup, update and integrity-check scripts.** `vesica check` is stubbed; the
  real implementation is M7.
- **Performance tuning.** Correctness first. The 100k-Entity targets are M7's
  problem, though nothing here should make them unreachable.

## Further Notes

**Three open decisions touch this spec**, and pretending otherwise would produce
rework:

- [Ticket 002](../wayfinder/tickets/002-build-and-test-loop.md) — where the build
  runs is unresolved and shapes every session of this work. It should be settled
  before implementation starts, not during.
- [Ticket 003](../wayfinder/tickets/003-product-name.md) — the name is a
  placeholder that this spec spreads into crate names. Settling it first is
  cheap; settling it after M0 is a rename across the workspace, and after M7 a
  rename across live service configuration.
- [Ticket 004](../wayfinder/tickets/004-schema-catalogue.md) — the starter Schema
  set above is provisional. The registry mechanism is not affected by the
  outcome; the YAML is replaced wholesale, which is the entire reason the
  ontology is data rather than code.

**On the checksum-warns-rather-than-refuses decision**: this will feel wrong to
an implementer with a validation habit. It is correct here. An investigator
copying a tax number from a scanned filing needs to record what the filing says,
including when the filing is wrong; a system that refuses the value forces them
to record it somewhere the system cannot see, which is worse than recording it
flagged.

**On testing against a real database**: the instinct to mock the store should be
resisted. SQLite in a temp directory is fast enough that a full suite stays under
a few seconds, and a fake that does not enforce foreign keys, cascades or
transactions hides precisely the class of bug this spec exists to prevent.

**On the audit log**: it is append-only by having no update or delete code path
at all. If a future requirement seems to need one, that is a signal to add a
compensating entry rather than to add the capability.
