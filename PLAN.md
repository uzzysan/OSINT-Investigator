# Development Plan

**The living document.** Every session updates it: check boxes, move things
between sections, record what was learned. If the plan and the code disagree,
the plan is wrong and gets fixed.

- **Status**: M0 complete; M1 well under way — entities and relations round-trip with full audit
- **Last updated**: 2026-09-10
- **Current milestone**: M1 — Domain and storage. Tickets 03, 06, 07, 08, 09, 10 and 11
  done; 12, 13 and 15 are on the frontier and independent of each other, and 14 is now
  unblocked.

---

## 0. How to read this

Each milestone has an **exit condition** — a demonstrable thing, not a feeling.
A milestone is done when its exit condition holds and its boxes are ticked.

Legend: `[ ]` not started · `[~]` in progress · `[x]` done · `[!]` blocked

Requirement ids (`F-ENT-1`) refer to
[`docs/02-functional-spec.md`](docs/02-functional-spec.md).

Milestones are turned into agent-ready specs in [`docs/specs/`](docs/specs/) as
work approaches them. M0 and M1 are covered by
[spec 0001](docs/specs/0001-foundation-and-domain-core.md).

---

## Progress at a glance

| Milestone | Theme | Status | Exit condition |
| --- | --- | --- | --- |
| M0 | Foundations | `[x]` **done** | `cargo test` and `install.sh` both run green on an empty app |
| M1 | Domain and storage | `[~]` in progress | Entities and relations round-trip through SQLite with full audit |
| M2 | API and MCP skeleton | `[ ]` | An agent creates a case and 20 entities through MCP alone |
| M3 | Web UI and canvas | `[ ]` | A human builds a 50-entity case in the browser without docs |
| M4 | Evidence and timeline | `[ ]` | A sourced, dated case renders a correct timeline |
| M5 | Entity resolution | `[ ]` | A duplicate across two cases surfaces in <5s with an explanation |
| M6 | Import, export, reports | `[ ]` | A CSV becomes a case; the case becomes a Markdown report |
| M7 | Deployment and hardening | `[ ]` | Clean-box install in <10 min, backup and restore verified |
| M8 | Polish and v1.0 | `[ ]` | All v1 success criteria in `docs/01` demonstrably met |

---

## M0 — Foundations

*Goal: a repository that builds, tests, lints and deploys an application that
does nothing. Everything after this is filling it in.*

- [x] Design documentation set (`docs/00`–`docs/09`, ADRs 0001–0004)
- [x] `.gitignore`, `.env.example`, git repository initialised
- [x] [Spec 0001](docs/specs/0001-foundation-and-domain-core.md) written for M0 + M1 (75 user stories, seams chosen)
- [x] Spec 0001 broken into [15 agent-ready tickets](docs/specs/0001/issues/) with blocking edges
- [x] [Design system](docs/10-design-system.md) — brand mark, light/dark tokens, type scale, components, motion, mobile
- [x] Cargo workspace with the eight crates from `docs/03-architecture.md`
- [x] `vesica` binary: config loading from env + `.env`, `tracing` setup, CLI
      subcommands stubbed (`serve`, `mcp`, `migrate`, `backup`, `check`)
- [x] axum server binding a configurable port, `/api/v1/health` and `/ready`
- [x] SQLite connection pool, WAL pragmas, `sqlx` migration runner on startup
- [x] Vite + React + TypeScript scaffold in `web/`, `rust-embed` wiring, dev proxy
- [x] `justfile`: `dev`, `build`, `test`, `lint`, `fmt`, `audit`
- [x] CI: fmt, clippy `-D warnings`, test, `cargo deny`, `gitleaks`, web build
- [x] `deploy/install.sh` that gets this far end to end
- [x] `docs/CONTRIBUTING.md` and `STATUS.md`

**Exit**: MET on 2026-09-09 — 160 tests green, fmt/clippy/check-deps clean, the
installer runs end to end and the service answers `/api/v1/health` and
`/api/v1/ready`.

---

## M1 — Domain and storage

*Goal: the data model, correct and tested, with no HTTP in sight.*

- [x] `vesica-core`: `PropertyType` enum with parse + validate per type
- [x] `vesica-core`: normalisation — names, emails, phones (E.164), domains, URLs,
      identifiers with checksums (IBAN, NIP, PESEL, VIN), partial dates
- [x] `vesica-core`: schema registry loading embedded YAML; inheritance resolution
- [~] Author the built-in entity schemas (provisional starter set of 11 + 6 shipped; full catalogue pending ticket 004) and ~15 relation schemas
- [x] `vesica-core`: caption derivation, ULID ids, validation errors carrying `fix`
- [x] Migrations 0001–0006 implementing `docs/04-data-model.md`
- [~] `vesica-store`: repositories for case, entity, relation, source, blob, audit
      (case, entity + `entity_prop`, relation + `relation_prop`, tag, audit and source all
      done as executor-generic function modules — no repository traits remain; no blob repo
      yet, and blobs stay out of scope until M4)
- [x] Single-writer task + reader pool; every multi-step write in a transaction
- [x] FTS5 table and sync triggers
- [~] Graph queries: neighbourhood CTE (bounded neighbours done; shortest path and components pending), shortest path, connected components,
      each with a depth limit, a node budget and a `truncated` flag
- [x] Audit log written on every mutation, in the same transaction, with `batch_id` grouping
- [x] Soft delete, restore, purge (purge redacts the target's earlier audit payloads — docs/09-security-and-legal.md)
- [x] Sources, citations at entity/relation/property-value granularity, and the unsourced report
- [x] Property-based tests for normalisation; golden tests for the schema registry

**Exit**: a test creates a case, 1,000 entities and 3,000 relations, queries
neighbours at depth 3, and asserts the audit log is complete.

---

### M1 progress

- [x] Ticket 08 — case lifecycle, the service layer, and the fixture later
      tickets build their tests on
- [x] Ticket 09 — entities, properties and tags; `repo::entity` converted to
      executor-generic functions and extended to `entity_prop`; `repo::tag` added
- [x] Ticket 10 — relations with properties and intervals; the property pipeline
      generalised over a `PropertyHost` trait so entity and relation values are
      normalised by the same code; entity deletion cascades to relations under a
      shared `batch_id` and restore undoes exactly that batch
- [x] Ticket 11 — sources, citations and unsourced flagging; `citation` is
      authoritative and the `*_prop.source_id` columns are a derived cache of the
      earliest surviving citation, which keeps the unsourced report an indexed
      lookup; `repo::source` converted, so no repository traits remain
- [ ] 12 search · [ ] 13 traversal · [ ] 14 audit history · [ ] 15 custom schemas
      — **all four are on the frontier now and independent of each other**

---

## M2 — API and MCP skeleton

*Goal: agents can use the thing before humans can.*

- [ ] DTOs separate from domain types; RFC 9457 problem responses with `fix`
- [ ] Case, entity, relation CRUD endpoints (`docs/06-http-api.md`)
- [ ] Graph and neighbours endpoints
- [ ] Search endpoint: FTS + fuzzy pass
- [ ] Schema endpoints
- [ ] `X-Actor` handling; `Idempotency-Key` support
- [ ] Auth middleware: `solo` and `token` modes, one layer, every route
- [ ] SSE `/events` with change publication from the write path
- [ ] `utoipa` OpenAPI generation, Scalar UI at `/api/docs`
- [ ] `vesica-mcp` on `rmcp`: discovery, read and write tools
- [ ] `graph_write` batch tool with `ref` resolution, transactional
- [ ] stdio transport (`vesica mcp`) and streamable HTTP at `/mcp`
- [ ] MCP resources (`case://`, `entity://`) and the five prompts
- [ ] Integration tests driving the MCP server as a client

**Exit**: a scripted agent session creates a case, writes 20 entities and 15
relations in two `graph_write` calls, and reads back a correct neighbourhood —
using only the tool list, with no other documentation.

---

## M3 — Web UI and canvas

*Goal: the part people judge the product by.*

- [ ] Ship the design tokens from [`docs/10-design-system.md`](docs/10-design-system.md) as CSS custom properties + Tailwind theme; self-host and subset the three font families
- [ ] App shell: case switcher, navigation, command palette, keyboard shortcuts
- [ ] Case list and case dashboard
- [ ] Graph canvas: Sigma.js + graphology, WebGL rendering
- [ ] ForceAtlas2 layout in a Web Worker; pin/unpin; persisted manual positions
- [ ] Alternative layouts: hierarchical, circular, grid
- [ ] Node and edge styling by schema, confidence and selection
- [ ] Selection, box-select, expand N hops, collapse, hide, focus
- [ ] Inspector panel: view and edit entity/relation properties in place
- [ ] Entity creation flow: schema picker, dynamic property form by type
- [ ] Canvas filters (schema, tag, confidence, date, text) preserving positions
- [ ] Path finding UI between two selected nodes
- [ ] Table view with virtualised rows and per-schema columns
- [ ] Global search UI with case attribution
- [ ] Tags with colours; bulk edit
- [ ] Saved views
- [ ] PNG/SVG canvas export
- [ ] Live updates from `/events`

**Exit**: someone who has not read the documentation builds a 50-entity case
with relations and finds a connection between two nodes.

---

## M4 — Evidence and timeline

- [ ] Source creation: url / file / manual / agent, with `retrieved_at`
- [ ] Multipart upload streaming to a temp file, SHA-256, content-addressed store
- [ ] Blob deduplication; thumbnails for images; first-page preview for PDFs
- [ ] Citations on entities, relations, property values and observations
- [ ] Unsourced-fact flagging in the UI and on the dashboard
- [ ] Source list view per case with dead-link detection
- [ ] Observations with partial dates and precision
- [ ] `timeline_event` materialisation from observations, intervals and date props
- [ ] Timeline UI: zoomable axis, precision-aware bands, grouping, filtering
- [ ] Timeline ↔ canvas selection linkage
- [ ] Time scrubbing that dims out-of-window graph elements
- [ ] Optional single-file HTML snapshot of URL sources

**Exit**: a case with 30 dated, sourced facts renders a timeline in the right
order with correct precision, and scrubbing the range changes the canvas.

---

## M5 — Entity resolution

- [ ] `block_key` generation for all key kinds; noise-key suppression
- [ ] `match_job` queue and background worker with backpressure
- [ ] Candidate shortlisting with schema-compatibility gating
- [ ] Feature extraction and the weighted scorer; weights in configuration
- [ ] `match_candidate` persistence with a JSON feature breakdown
- [ ] Explanation rendering — prose, not a score
- [ ] Review queue UI: sorted, filterable, keyboard-driven
- [ ] Merge (same case) with full undo under one `batch_id`
- [ ] Same-as link (cross case) with `entity_link`
- [ ] "Different" tombstones; "defer" with expiry
- [ ] Cross-case traversal: foreign nodes on the canvas, `follow_links` in queries
- [ ] `vesica rescan-matches`
- [ ] MCP: `match_queue`, `match_find`, `match_resolve` (merge gated by config)
- [ ] Labelled fixture set (~300 pairs); precision/recall/F1 report per threshold
- [ ] Calibrate default thresholds against the fixture set

**Exit**: adding an entity that duplicates one in another case surfaces a scored,
explained suggestion within 5 seconds; merge and undo both work; a rejected pair
is never proposed again.

---

## M6 — Import, export and reports

- [ ] CSV / TSV / XLSX parsing with encoding and delimiter detection
- [ ] Column-to-property mapping UI with a suggested mapping
- [ ] Dry-run: what would be created, what would match existing entities
- [ ] Commit under one `batch_id`; undo an import as a unit
- [ ] JSON export and import of a whole case
- [ ] FollowTheMoney JSON-lines export and import
- [ ] Markdown report generator: summary, dossiers, relation tables, timeline,
      Mermaid subgraph, source bibliography
- [ ] `case_report` and `case_export` MCP tools; `subgraph_render`
- [ ] `vesica import` / `vesica export` headless commands

**Exit**: a 500-row CSV becomes a correctly typed case in under a minute, and
that case exports to a Markdown report a person would actually send.

---

## M7 — Deployment and hardening

- [ ] `install.sh`: preflight, toolchain detection and optional install, build,
      free-port discovery, `.env` generation, data dir, migrate, systemd, verify
- [ ] `--port`, `--bind`, `--data-dir`, `--auth`, `--no-service`, `--no-install`,
      `--dry-run`
- [ ] systemd unit with the hardening options from `docs/08-deployment.md`
- [ ] `update.sh` with pre-update backup and automatic rollback on failed health
- [ ] `backup.sh` / `vesica backup` using SQLite's online backup API; retention
- [ ] `vesica check`: blob hashes, orphans, foreign keys
- [ ] Restore procedure, documented and rehearsed
- [ ] macOS launchd path; Alpine/musl target; arm64 build
- [ ] Dockerfile and compose.yml (secondary path)
- [ ] Prebuilt release binaries in CI for x86-64 and arm64
- [ ] Reverse-proxy examples (Caddy, nginx)
- [ ] CSP, security headers, upload limits, rate limits, request timeouts
- [ ] Clean-box install test in CI on Debian and Ubuntu images

**Exit**: on a fresh cloud VM, clone to working service in under 10 minutes with
no manual steps; a backup taken under load restores to an identical database.

---

## M8 — Polish and v1.0

- [ ] Onboarding: a demo case that loads on first run and can be deleted
- [ ] Empty states that explain what to do next
- [ ] Custom schema editor UI
- [ ] Performance pass against the 20k-node / 60k-edge target
- [ ] Accessibility pass on the core flows
- [ ] User guide, agent guide, troubleshooting guide
- [ ] `ETHICS.md`; licence decision (open ticket)
- [ ] `CHANGELOG.md`; release process; version stamping in `/api/v1/stats`
- [ ] All `docs/01` success criteria verified and recorded

---

## Working agreements

1. **Backups before edits.** Any script that rewrites existing files takes a copy
   into `.backups/` first.
2. **Commit per meaningful change**, with a short factual message. No
   end-of-session mega-commits.
3. **Secrets live in `.env`.** `.env.example` stays current. `gitleaks` runs in
   CI. Never a credential in a commit, a log, or an export.
4. **Update this file in the same commit** as the work it describes.
5. **`STATUS.md` is the session log** — where work stopped, what is in flight,
   what the next action is. Written before stopping, always.
6. **A milestone is not done until its exit condition is demonstrated**, not
   until its boxes look ticked.
7. **Tests accompany the code**, not a later cleanup pass. `vesica-core` and
   `vesica-resolve` in particular are pure and have no excuse.
8. **Check `docs/wayfinder/map.md`** before starting: open decisions are listed
   there, and building past one is how rework happens.

---

## Deferred to after v1

Recorded so they are not rediscovered as ideas: collector/enricher plugin
runner · document OCR and full-text document indexing · geospatial map view ·
multi-user accounts and RBAC · real-time collaborative editing · graph ML and
link prediction · mobile applications · federation between instances.
