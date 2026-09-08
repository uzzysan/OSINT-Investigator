# Wayfinder Map — Vesica

The route from a loose idea to a shipped v1. Tickets here hold **decisions**;
[`PLAN.md`](../../PLAN.md) holds **execution**. A decision that is still open is
a place where building would cause rework.

Tracker: local markdown. Tickets are files in
[`tickets/`](tickets/); an open ticket is one whose front matter says
`status: open`.

---

## Destination

A running v1 of Vesica on a dedicated machine: a single-binary,
self-hosted, case-based investigative knowledge graph with cross-case entity
resolution, a timeline, a graph canvas and an MCP server, installed by cloning
the repository and running one script. Every design and progress document lives
in `.md` files in this repository, kept current as the work proceeds.

## Notes

- **Domain**: OSINT / investigative link analysis. Reference points: Maltego
  (canvas and pivoting), OCCRP Aleph + FollowTheMoney (ontology),
  OpenSanctions `nomenklatura` (entity resolution).
- **This map carries execution.** Unlike a pure planning map, the destination is
  a running system, so the plan in `PLAN.md` is worked as well as charted.
- **Skills to consult**: `engineering:architecture` for ADRs,
  `engineering:system-design` for component decisions, `engineering:testing-strategy`
  before M1, `design:design-critique` before M3 ships, `research` for anything
  needing outside facts.
- **Standing preferences**: Rust first, Python second; open-source tooling,
  Rust-native where a credible option exists. Backups before edits. Git commit
  per meaningful change. Secrets in `.env`, never in the repository. Keep the
  status log current so work can resume mid-stream.
- **Build loop**: agents build and test in the cloud container; the device folder
  is the source of truth and is synced and committed once per ticket. Settled in
  [ticket 002](tickets/002-build-and-test-loop.md).

## Decisions so far

- [Storage and packaging](../adr/0001-single-binary-sqlite.md): single Rust
  binary with embedded SQLite (WAL, FTS5, recursive-CTE traversal), a repository
  seam left open for Postgres. Chosen because one-command deployment is the
  dominant constraint.
- [Ontology](../adr/0002-ftm-inspired-ontology.md): a curated ~30-schema subset
  of FollowTheMoney using its ids, property names and property types, extensible
  with custom schemas per deployment, exportable as FtM JSON-lines.
- [Collection](../adr/0003-agent-fed-collection.md): no built-in collectors in
  v1. Agents run their own tooling and write findings through MCP; bulk import
  covers existing datasets.
- [Access](../adr/0004-solo-mode-access.md): solo mode, no login, bound to
  localhost by default, with a shared-token mode and a single middleware seam so
  real accounts stay possible.
- Frontend stack: React + TypeScript + Vite, Sigma.js v3 + graphology for the
  canvas, TanStack Query/Table, shadcn/ui. Recorded in
  [`docs/03-architecture.md`](../03-architecture.md#35-frontend).
- MCP implementation: `rmcp`, the official Rust SDK, serving stdio and
  streamable HTTP from the same process as the web server.
- [Build and test loop](tickets/002-build-and-test-loop.md): agents build and
  test in the cloud container (cargo already present, more memory, no shell time
  limit); the device folder stays the single source of truth, synced and
  committed once per ticket.
- [Product name](tickets/003-product-name.md): **Vesica** — the lens two
  overlapping circles make, which is what the logomark is and what the product
  does. Applied to crates, binary, service, paths, environment prefix and the
  MCP server id.

Specs derived from these decisions live in [`docs/specs/`](../specs/). A spec
names the tickets it depends on rather than assuming them closed.

## Open tickets

| # | Ticket | Type | State |
| --- | --- | --- | --- |
| 001 | [Licence and distribution model](tickets/001-licence.md) | grilling | frontier |
| 004 | [Built-in schema catalogue](tickets/004-schema-catalogue.md) | grilling | frontier |
| 005 | [Canvas performance at target scale](tickets/005-canvas-performance.md) | prototype | frontier |
| 006 | [Timeline rendering approach](tickets/006-timeline-rendering.md) | prototype | blocked by 005 |
| 007 | [Entity-resolution calibration set](tickets/007-er-calibration-set.md) | research | blocked by 004 |

## Not yet specified

In scope, not yet sharp enough to ticket. Graduates as the frontier advances.

- **Custom schema editor UX.** We know deployments need custom entity types
  (F-ENT-8). What that looks like — a form, a YAML editor, an import — is
  unclear until the built-in schema catalogue (004) settles and M3 shows how the
  property forms behave.
- **Report templating.** `case_report` needs a shape. Whether that is a fixed
  set of sections, a template language, or agent-composed Markdown depends on
  what M6 users actually ask for.
- **Agent autonomy on merges.** `VESICA_MCP_ALLOW_MERGE` exists as a flag, but the
  right default, and whether an agent-proposed merge should queue for human
  approval rather than being refused, needs M5 experience.
- **Behaviour above the canvas ceiling.** Sampling, server-side layout,
  level-of-detail — the answer depends on what 005 measures.
- **URL snapshotting.** F-SRC-6 is a "should"; the storage cost, the fidelity and
  the legal posture of archiving third-party pages all need thought before it is
  specified.
- **Undo depth.** Merge and import undo are specified. Whether general
  property-level undo is worth its complexity is open until the audit log is real.

## Out of scope

Ruled beyond the destination. These return only if the destination is redrawn.

- **Built-in collectors / transform runner** — [ADR-0003](../adr/0003-agent-fed-collection.md).
  Agents cover it; the plugin seam is noted but unbuilt.
- **Multi-user accounts, RBAC, SSO** — [ADR-0004](../adr/0004-solo-mode-access.md).
  Solo mode is the target; the seam is preserved.
- **Multi-tenant hosted service** — a different product.
- **Document OCR and corpus-scale document search** — Aleph's core competence;
  attachments here are evidence, not a searchable corpus.
- **Real-time collaborative editing** — only meaningful with multi-user.
- **Native mobile applications** — the responsive web UI is the answer.
- **Graph ML / link prediction** — needs data volume that does not exist yet.
