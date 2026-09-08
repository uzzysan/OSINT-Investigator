# 3. Architecture

## 3.1 Shape of the thing

One process. It serves the web UI, the REST API and the MCP endpoint from the
same port, and talks to one SQLite file and one blob directory on local disk.

```
                    ┌──────────────────────────────────────────────┐
 Browser  ─────────▶│                                              │
                    │              osintd  (one binary)            │
 AI agent ─────────▶│                                              │
 (HTTP /mcp)        │  ┌────────────────────────────────────────┐  │
                    │  │ axum router                            │  │
 AI agent ─────────▶│  │  /            → embedded SPA assets    │  │
 (stdio)            │  │  /api/v1/*    → REST                   │  │
                    │  │  /mcp         → MCP streamable HTTP    │  │
                    │  │  /events      → SSE (live updates)     │  │
                    │  │  /blob/*      → attachments            │  │
                    │  └───────────────┬────────────────────────┘  │
                    │                  │                           │
                    │  ┌───────────────▼────────────────────────┐  │
                    │  │ osint-core   domain, schema registry,  │  │
                    │  │              validation, normalisation │  │
                    │  └───────────────┬────────────────────────┘  │
                    │                  │                           │
                    │  ┌───────────────▼──────┐  ┌──────────────┐  │
                    │  │ osint-store          │  │ osint-resolve│  │
                    │  │ sqlx + SQLite (WAL)  │◀─┤ matcher      │  │
                    │  │ FTS5, migrations     │  │ (bg worker)  │  │
                    │  └───────────────┬──────┘  └──────────────┘  │
                    └──────────────────┼───────────────────────────┘
                                       │
                          ┌────────────▼─────────────┐
                          │ data/osint.sqlite        │
                          │ data/blobs/<sha256>      │
                          └──────────────────────────┘
```

Everything that is not the database file or the blob directory is disposable.
Rebuild the binary, restart, and the state is intact.

## 3.2 Why this shape

Recorded properly in the ADRs; the short version:

- **Single binary + SQLite** ([ADR-0001](adr/0001-single-binary-sqlite.md)) —
  the deployment requirement ("clone, run a script, service comes up on a free
  port") is the dominant constraint. Every alternative adds a daemon to install,
  configure, secure, monitor and back up. SQLite in WAL mode handles one writer
  and many readers, which is exactly the load profile of one analyst plus a few
  agents. FTS5 gives full-text; recursive CTEs give traversal.
- **Rust** — the operator's stated preference, and it happens to fit: a static
  binary with no runtime to install is what makes the deploy story work.
- **Embedded web assets** — the SPA is compiled into the binary with
  `rust-embed`, so there is no static-file directory to get out of sync with the
  build.

## 3.3 Cargo workspace

```
crates/
  osint-core/       # no I/O. Domain types, schema registry, property types,
                    # normalisation, validation, caption derivation, IDs.
  osint-store/      # sqlx/SQLite. Migrations, repositories, FTS, graph queries.
  osint-resolve/    # blocking keys, similarity, scoring, candidate queue.
  osint-api/        # axum handlers, DTOs, OpenAPI, SSE, upload handling.
  osint-mcp/        # rmcp server: tools, resources, prompts. Calls osint-core
                    # + osint-store through the same service layer as the API.
  osint-report/     # Markdown / JSON / FtM rendering.
  osint-import/     # CSV/XLSX/FtM ingestion with mapping and dry-run.
  osintd/           # the binary: config, wiring, CLI subcommands, systemd glue.
web/                # Vite + React + TypeScript SPA
migrations/         # numbered SQL files, applied at startup
deploy/             # install.sh, systemd unit, backup.sh, update.sh
```

The dependency direction is strict and one-way:

```
osintd → osint-api, osint-mcp
osint-api, osint-mcp → osint-report, osint-import, osint-resolve, osint-store
osint-resolve, osint-store, osint-import, osint-report → osint-core
osint-core → nothing internal
```

`osint-core` must stay I/O-free so the domain rules are testable without a
database. Everything the API can do, MCP can do, because both call the same
service functions rather than each other.

## 3.4 Crate-by-crate

### osint-core
Pure domain. Owns:
- `Schema`, `PropertyDef`, `PropertyType`, `SchemaRegistry` (built-ins loaded
  from an embedded YAML file, custom ones layered on from the database).
- Value normalisation per property type — the single place phone numbers become
  E.164 and names get folded. Used by both writes and the matcher, so they can
  never disagree.
- `Caption` derivation from a schema's template.
- ID generation: ULIDs, so ids sort by creation time and are safe to generate
  client-side or agent-side.
- Validation errors that carry a fix suggestion, which the MCP layer surfaces
  verbatim to agents.

### osint-store
- `sqlx` with the SQLite driver, compile-time-checked queries, migrations run at
  startup inside a transaction.
- Repositories: `CaseRepo`, `EntityRepo`, `RelationRepo`, `SourceRepo`,
  `BlobRepo`, `MatchRepo`, `AuditRepo`.
- Graph queries as recursive CTEs: neighbourhood expansion, shortest path
  (bidirectional BFS in SQL, capped by depth and node budget), connected
  components.
- FTS5 virtual table kept in sync by triggers.
- One writer connection, a read pool. `busy_timeout`, `WAL`, `synchronous=NORMAL`,
  `foreign_keys=ON`.

### osint-resolve
Background worker fed by a durable queue table. For each dirty entity it
computes blocking keys, shortlists candidates, scores them and writes
`match_candidate` rows. Never mutates entities. Detail in
[`05-entity-resolution.md`](05-entity-resolution.md).

### osint-api
Thin. Handlers parse, call a service, serialise. DTOs are separate from domain
types so the wire format can be stable while the domain evolves. Uploads stream
to a temp file, hash, then move into the content-addressed store. SSE stream
publishes change events so open browser tabs update when an agent writes.

### osint-mcp
Built on [`rmcp`](https://docs.rs/rmcp), the official Rust MCP SDK. Tools are
declared with `#[tool]` and a `tool_router`, giving JSON Schema for free. The
same `Arc<Services>` is shared with the HTTP layer. `osintd mcp` runs it over
stdio; the HTTP server mounts the streamable-HTTP service at `/mcp`.

### osintd
Config from environment plus `.env`, CLI subcommands:

| Command | Purpose |
| --- | --- |
| `osintd serve` | Run the web + API + MCP HTTP server (default) |
| `osintd mcp` | Run the MCP server over stdio |
| `osintd migrate` | Apply migrations and exit |
| `osintd rescan-matches` | Re-run entity resolution over everything |
| `osintd backup [--out DIR]` | Consistent backup of DB + blob manifest |
| `osintd check` | Integrity check: blob hashes, orphans, FK validation |
| `osintd import <file>` | Headless import |
| `osintd export <case>` | Headless export |

## 3.5 Frontend

| Concern | Choice | Reason |
| --- | --- | --- |
| Framework | React 19 + TypeScript, Vite | Largest component ecosystem for the panel-heavy UI this needs |
| Graph rendering | **Sigma.js v3 + graphology** | WebGL; comfortably renders tens of thousands of nodes, which Cytoscape.js (SVG/canvas) does not |
| Layout | `graphology-layout-forceatlas2` in a Web Worker | Keeps the main thread responsive during layout |
| Graph analytics | `graphology-metrics`, `graphology-communities-louvain` | Degree/betweenness/communities client-side on the loaded subgraph |
| State/data | TanStack Query + Zustand | Server cache vs. canvas UI state, kept separate |
| Components | shadcn/ui + Tailwind | Dense, keyboard-friendly, no design system to invent |
| Timeline | `vis-timeline` or a custom D3 scale | Decision open — [ticket 006](wayfinder/tickets/006-timeline-rendering.md) |
| Tables | TanStack Table | Virtualised rows for large cases |

Built with `vite build`, output embedded into the binary via `rust-embed`. A dev
mode proxies to the Rust server so the SPA can hot-reload.

## 3.6 Request and event flow

**Human adds an entity**

1. `POST /api/v1/cases/{case}/entities` with schema + properties.
2. `osint-core` validates, normalises, derives the caption.
3. `osint-store` writes `entity` + `entity_prop` + `audit_log` + an FTS row and
   enqueues a `match_job` — all in one transaction.
4. Response returns immediately.
5. The resolver worker picks up the job, scores candidates, writes
   `match_candidate` rows and publishes an SSE event.
6. Open browsers show the new suggestion badge without a refresh.

**Agent bulk-writes via MCP**

1. Agent calls `graph_batch_write` with entities and relations.
2. One transaction; entity references inside the batch are resolved by temporary
   local ids so relations can point at entities created in the same call.
3. Actor recorded as the agent's declared name.
4. One match job per created entity; the same SSE event stream fires.

## 3.7 Concurrency model

SQLite allows one writer. Writes go through a single writer task fed by a
channel; reads use a connection pool. This is simpler than optimistic retries
and makes lock contention impossible rather than merely unlikely. The resolver
worker batches its writes and yields to interactive traffic.

If write throughput ever becomes the limit — it will not, at this scale — the
`osint-store` repository trait is the seam behind which Postgres could be added.
That seam is deliberate but unused; see [ADR-0001](adr/0001-single-binary-sqlite.md).

## 3.8 Observability

- `tracing` with structured spans; text to stdout by default, JSON on request.
- `/api/v1/health` (liveness) and `/api/v1/ready` (migrations applied, DB
  writable, blob dir writable).
- `/api/v1/stats` for counts, queue depth and resolver lag.
- Slow-query log above a configurable threshold.
