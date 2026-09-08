# Spec 0001 — tickets

Tracer-bullet vertical slices of
[Spec 0001](../../0001-foundation-and-domain-core.md). Numbered in dependency
order, so a ticket's blockers always have lower numbers than it does. Each is
sized for a single fresh context window and is verifiable on its own.

All carry the `ready-for-agent` status: they are agent-grabbable by construction,
and need no further triage.

| # | Ticket | Blocked by |
| --- | --- | --- |
| 01 | [Cargo workspace and crate skeleton](01-cargo-workspace-and-crate-skeleton.md) | — |
| 02 | [Configuration and structured logging](02-configuration-and-structured-logging.md) | 01 |
| 03 | [SQLite store, migrations and single-writer runtime](03-sqlite-store-migrations-and-single-writer-runtime.md) | 02 |
| 04 | [Health, readiness and embedded web scaffold](04-health-readiness-and-embedded-web-scaffold.md) | 03 |
| 05 | [CI pipeline and one-command install](05-ci-pipeline-and-one-command-install.md) | 04 |
| 06 | [Property types, normalisation and partial dates](06-property-types-normalisation-and-partial-dates.md) | 01 |
| 07 | [Schema registry — the ontology as data](07-schema-registry-ontology-as-data.md) | 06 |
| 08 | [Case lifecycle](08-case-lifecycle.md) | 03 |
| 09 | [Entities, properties and tags](09-entities-properties-and-tags.md) | 07, 08 |
| 10 | [Relations with properties and intervals](10-relations-with-properties-and-intervals.md) | 09 |
| 11 | [Sources, citations and unsourced flagging](11-sources-citations-and-unsourced-flagging.md) | 10 |
| 12 | [Full-text search across Cases](12-full-text-search-across-cases.md) | 09 |
| 13 | [Bounded graph traversal](13-bounded-graph-traversal.md) | 10 |
| 14 | [Audit history and integrity check](14-audit-history-and-integrity-check.md) | 11 |
| 15 | [Custom schemas layered over built-ins](15-custom-schemas-layered-over-built-ins.md) | 09 |

## Dependency graph

```
01 ─┬─ 02 ── 03 ─┬─ 04 ── 05
    │            │
    │            └─ 08 ─┐
    │                   ├─ 09 ─┬─ 10 ─┬─ 11 ── 14
    └─ 06 ── 07 ────────┘      │      └─ 13
                               ├─ 12
                               └─ 15
```

## Working the frontier

Take any ticket whose blockers are all complete. At the start that is **01**
alone; once it lands, **02** and **06** open in parallel, and the two chains
(deployment on one side, domain on the other) run independently until they meet
at **09**.

Two edges worth understanding rather than assuming:

- **08 is not blocked by the schema registry.** A Case needs no Schemas, so it
  runs in parallel with 06 and 07. The cost is that the service layer's
  constructor gains a registry field in 09.
- **11 is blocked by 10, not 09.** Citing a Property value on a Relation requires
  Relations to exist.
