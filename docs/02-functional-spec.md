# 2. Functional Specification

Requirements are identified `F-<area>-<n>` so the plan and the tests can point at
them. Priority: **M** must-have for v1, **S** should-have, **C** could-have,
**W** won't-have this release.

---

## 2.1 Cases

| ID | Pri | Requirement |
| --- | --- | --- |
| F-CASE-1 | M | Create a case with name, slug, description, colour and optional tags. |
| F-CASE-2 | M | List, rename, archive and delete cases. Deleting is soft; a purge is a separate, confirmed action. |
| F-CASE-3 | M | A case dashboard shows entity counts by schema, relation counts, recent activity, unresolved match suggestions and open questions. |
| F-CASE-4 | M | Every entity, relation, source and note belongs to exactly one case. |
| F-CASE-5 | S | Case-level free-text "case notes" in Markdown, versioned. |
| F-CASE-6 | S | Export a whole case as JSON, FollowTheMoney JSON-lines, or a Markdown report bundle with attachments. |
| F-CASE-7 | S | Import a previously exported case, either as a new case or merged into an existing one. |
| F-CASE-8 | C | Case templates: pre-seed a new case with a standard set of entity stubs and questions (e.g. "company due diligence"). |

### Cross-case behaviour

Cases are isolated for *editing* and joined for *discovery*. A user working in
Case A never accidentally edits Case B's data, but the resolver and global
search see everything, and confirmed same-as links let the canvas pull a
neighbour from Case B in as a visually distinct "foreign" node.

---

## 2.2 Entities

| ID | Pri | Requirement |
| --- | --- | --- |
| F-ENT-1 | M | Create an entity by choosing a schema and filling properties. Only `caption` is mandatory; the system derives it from name-ish properties when omitted. |
| F-ENT-2 | M | Properties are typed and may repeat (a person can have five phone numbers). |
| F-ENT-3 | M | Property values are stored raw *and* normalised (see [data model](04-data-model.md#normalisation)). The raw value is never lost. |
| F-ENT-4 | M | Edit and delete entities. Delete is soft, restorable for 30 days, then purgeable. |
| F-ENT-5 | M | An entity shows: properties, relations in/out, sources, attachments, timeline, notes, tags, match suggestions, audit trail. |
| F-ENT-6 | M | Free-form tags with colours, filterable. |
| F-ENT-7 | M | Confidence flag per entity: `confirmed`, `probable`, `hypothesis`. Rendered distinctly on the canvas. |
| F-ENT-8 | S | Custom schemas: define a new entity type with its own properties and icon, per deployment. |
| F-ENT-9 | S | Custom properties on a built-in schema without forking it. |
| F-ENT-10 | S | Bulk edit: select many entities on the canvas or in the table and set tags, confidence or a property in one action. |
| F-ENT-11 | C | Entity "aliases": alternative names/spellings/transliterations as a first-class repeated property feeding the resolver. |

### Built-in schemas (v1)

Drawn from FollowTheMoney, trimmed to what investigations actually use daily.

**Actors** — `Person`, `Company`, `Organization`, `PublicBody`, `LegalEntity`
**Contact & identity** — `Address`, `Email`, `Phone`, `UserAccount`, `Identification`, `Passport`
**Digital** — `Website`, `Domain`, `IPAddress`, `CryptoWallet`
**Assets** — `BankAccount`, `Vehicle`, `Vessel`, `Airplane`, `RealEstate`, `Security`
**Evidence & events** — `Document`, `Image`, `Event`, `Note`
**Legal** — `CourtCase`, `Contract`, `Sanction`, `License`

Each ships with a stable id, a display icon, a colour, a caption template and an
ordered property list. Full definitions in [`04-data-model.md`](04-data-model.md).

---

## 2.3 Relations

| ID | Pri | Requirement |
| --- | --- | --- |
| F-REL-1 | M | Create a typed, directed relation between two entities. |
| F-REL-2 | M | Relations carry properties of their own (`role`, `percentage`, `amount`, `currency`, `note`). |
| F-REL-3 | M | Relations carry an optional validity interval with a stated precision (`year`, `month`, `day`, `instant`). |
| F-REL-4 | M | A relation shows its sources and its own audit trail. |
| F-REL-5 | M | Schema constraints: `Ownership` connects a `LegalEntity` to a `Thing`; the UI only offers legal combinations but allows an explicit override with a warning. |
| F-REL-6 | S | Custom relation schemas. |
| F-REL-7 | S | Undirected relations (`Associate`, `Colocated`) rendered without arrowheads. |

### Built-in relation schemas (v1)

`Ownership`, `Directorship`, `Employment`, `Membership`, `Family`,
`Representation`, `Payment`, `Registration`, `Residence`, `Colocation`,
`Communication`, `Associate`, `Mention`, `SameAs` (system-managed), `Custom`.

---

## 2.4 Sources, evidence and attachments

| ID | Pri | Requirement |
| --- | --- | --- |
| F-SRC-1 | M | Attach a source to an entity, a relation, a property value or an observation. |
| F-SRC-2 | M | Source kinds: `url`, `file`, `manual`, `agent`. Each carries a title, capture timestamp, and optional excerpt. |
| F-SRC-3 | M | Upload attachments: images, PDFs, office documents, plain text, archives. Stored content-addressed by SHA-256; identical uploads deduplicate. |
| F-SRC-4 | M | Image attachments get thumbnails; PDFs get a first-page preview. |
| F-SRC-5 | M | Every source records `retrieved_at` and, for URLs, the URL as given plus a normalised form. |
| F-SRC-6 | S | Optional local snapshot of a URL (single-file HTML) captured at add time, so link rot doesn't erase the evidence. |
| F-SRC-7 | S | A per-case source list view: everything cited, what cites it, and dead-link flags. |
| F-SRC-8 | C | EXIF extraction from images into properties (capture time, camera, GPS) with an explicit opt-in per upload. |

Facts with **no** source are permitted but visually flagged, and the case
dashboard counts them.

---

## 2.5 Graph canvas

| ID | Pri | Requirement |
| --- | --- | --- |
| F-GRAPH-1 | M | WebGL canvas rendering the case graph; pan, zoom, box-select, multi-select. |
| F-GRAPH-2 | M | Node appearance encodes schema (icon + colour), confidence (border style) and selection. Edge appearance encodes relation schema and direction. |
| F-GRAPH-3 | M | Force-directed layout computed in a web worker, with a stop/pin control and manual node positioning that persists. |
| F-GRAPH-4 | M | Alternative layouts: hierarchical, circular, grid, and "organic". |
| F-GRAPH-5 | M | Expand a node to N hops; collapse; hide; focus (isolate a node's neighbourhood). |
| F-GRAPH-6 | M | Filter the canvas by schema, tag, confidence, date range and free text, without losing positions. |
| F-GRAPH-7 | M | Shortest path and all-paths-up-to-N between two selected entities, highlighted on the canvas. |
| F-GRAPH-8 | M | Inspector panel for the selected node or edge; edit in place. |
| F-GRAPH-9 | S | Saved views: named filter + layout + camera, per case. |
| F-GRAPH-10 | S | Foreign nodes: entities pulled in from another case via a confirmed same-as link, rendered with a distinct outline and a "jump to case" action. |
| F-GRAPH-11 | S | Graph metrics on demand: degree, betweenness, closeness, connected components, community detection; results usable as filters and as node size. |
| F-GRAPH-12 | S | Export the current canvas as PNG and SVG. |
| F-GRAPH-13 | C | Grouping / clustering nodes into a collapsible super-node. |
| F-GRAPH-14 | C | Free annotations on the canvas: text labels, boxes, arrows. |

Performance target: 20,000 nodes / 60,000 edges pans at 30+ fps on a mid-range
laptop. Above that, the canvas switches to a sampled view with an explicit
"showing N of M" banner and pushes the user toward filters.

---

## 2.6 Table and search

| ID | Pri | Requirement |
| --- | --- | --- |
| F-TBL-1 | M | Table view of entities in a case: sortable, filterable, column selection per schema, paginated. |
| F-TBL-2 | M | Global search across all cases: full-text over captions, property values, notes and source titles. |
| F-TBL-3 | M | Fuzzy search tolerant of misspellings and transliterations. |
| F-TBL-4 | M | Structured search: `schema:Person country:pl tag:target birthDate:1980..1985`. |
| F-TBL-5 | S | Search results grouped by case, showing which case each hit lives in. |
| F-TBL-6 | S | Save a search as a named query. |

---

## 2.7 Entity resolution and cross-case linking

Full algorithm in [`05-entity-resolution.md`](05-entity-resolution.md).

| ID | Pri | Requirement |
| --- | --- | --- |
| F-ER-1 | M | On every entity write, generate match candidates against all entities in all cases within seconds, asynchronously. |
| F-ER-2 | M | Each candidate carries a score in `[0,1]` and a structured explanation ("same normalised phone +48601234567; name similarity 0.94; both linked to ACME Sp. z o.o."). |
| F-ER-3 | M | Candidates above the suggest threshold appear in a per-case **review queue** and as a badge on the entity. |
| F-ER-4 | M | Nothing is ever merged or linked automatically. |
| F-ER-5 | M | Review actions: **Merge** (same case), **Link as same-as** (cross-case), **Mark as different** (suppresses the pair permanently), **Defer**. |
| F-ER-6 | M | Merge is reversible: it records the full pre-merge state and can be undone. |
| F-ER-7 | M | "Mark as different" is remembered and never re-proposed, even after edits. |
| F-ER-8 | S | Manual "find duplicates of this entity" on demand, with a lower threshold than the automatic pass. |
| F-ER-9 | S | Full re-scan command for when weights or schemas change. |
| F-ER-10 | S | Tunable weights and thresholds in configuration, with a preview of how many pairs a change would surface. |
| F-ER-11 | C | Transitive same-as clusters: A~B, B~C implies a three-member cluster view. |

---

## 2.8 Timeline

| ID | Pri | Requirement |
| --- | --- | --- |
| F-TIME-1 | M | Every date-bearing thing (entity property, relation interval, observation, source capture) contributes events to a case timeline. |
| F-TIME-2 | M | Date precision is stored and honoured: a "2019" event renders as a year-wide band, not a point on 1 January. |
| F-TIME-3 | M | Timeline view: zoomable horizontal axis, events as markers, grouped by entity or by schema, filterable. |
| F-TIME-4 | M | Click an event to select the underlying entity/relation; the canvas follows. |
| F-TIME-5 | S | Time scrubbing: a date-range slider that dims graph elements outside the window, so the graph can be "played" over time. |
| F-TIME-6 | S | Manual observations: "on 2024-03-11 the vehicle was seen at this address", with source. |
| F-TIME-7 | C | Export the timeline as a Mermaid `timeline`/`gantt` block for embedding in reports. |

---

## 2.9 Import and export

| ID | Pri | Requirement |
| --- | --- | --- |
| F-IO-1 | M | Import CSV/TSV/XLSX: preview, map columns to a schema's properties, choose the entity key, dry-run showing what would be created vs. matched, then commit. |
| F-IO-2 | M | Import runs are recorded and reversible as a unit. |
| F-IO-3 | M | Export a case or a selection as JSON. |
| F-IO-4 | S | Export as FollowTheMoney JSON-lines, so the data can go into Aleph or be checked against OpenSanctions. |
| F-IO-5 | S | Import FollowTheMoney JSON-lines and OpenSanctions datasets. |
| F-IO-6 | S | Markdown report generation: case summary, entity dossiers, relation tables, timeline, embedded Mermaid graph diagram, source bibliography. |
| F-IO-7 | C | GEXF / GraphML export for Gephi. |
| F-IO-8 | C | PDF report rendering. |

---

## 2.10 MCP server for AI agents

Full tool surface in [`07-mcp-server.md`](07-mcp-server.md).

| ID | Pri | Requirement |
| --- | --- | --- |
| F-MCP-1 | M | An MCP server exposing case, entity, relation, source, search, traversal, timeline and match tooling. |
| F-MCP-2 | M | Two transports: stdio for local agents, streamable HTTP at `/mcp` for remote ones, both served by the same process and the same code path. |
| F-MCP-3 | M | Every write records the agent's identity as the actor, so the audit log distinguishes human from machine edits. |
| F-MCP-4 | M | Tool descriptions and schemas are self-explanatory enough that an agent needs no external documentation. Errors explain how to fix the call. |
| F-MCP-5 | M | Bulk write tool: create many entities and relations in one transactional call, since agents work in batches. |
| F-MCP-6 | M | Read tools return compact, token-efficient representations by default, with an explicit `include` parameter to ask for more. |
| F-MCP-7 | S | MCP resources: `case://{id}/summary`, `entity://{id}` as readable documents. |
| F-MCP-8 | S | MCP prompts for common workflows: "profile this person", "write the case report", "triage the match queue". |
| F-MCP-9 | S | An agent can read the match queue and *propose* resolutions, but confirmation of a merge remains a human action by default (configurable). |
| F-MCP-10 | C | Idempotency keys so a retried agent call doesn't duplicate data. |

---

## 2.11 Audit, undo and data safety

| ID | Pri | Requirement |
| --- | --- | --- |
| F-AUD-1 | M | Append-only audit log: actor, action, target, before/after, timestamp. |
| F-AUD-2 | M | Per-entity history view. |
| F-AUD-3 | M | Undo for merges, imports and bulk edits. |
| F-AUD-4 | M | Nightly local backup of the database plus a blob manifest, with configurable retention. |
| F-AUD-5 | M | Integrity check command: verify blob hashes, find orphans, validate foreign keys. |
| F-AUD-6 | S | Export the audit log for a case as part of the report bundle. |

---

## 2.12 Non-functional requirements

| ID | Requirement |
| --- | --- |
| NF-1 | Cold start to serving requests in under 2 seconds. |
| NF-2 | Search and entity reads under 200 ms at 100k entities. |
| NF-3 | Match candidate generation for one new entity under 5 s at 100k entities. |
| NF-4 | Single process, single database file, no external services. Idle RSS under 150 MB. |
| NF-5 | Linux x86-64 and arm64 primary; macOS for development; Windows via WSL2. |
| NF-6 | No outbound network calls except those the operator explicitly configures. |
| NF-7 | Crash-safe: SQLite WAL, all multi-step writes in transactions. |
| NF-8 | The UI works in current Firefox and Chromium; keyboard-navigable core flows. |
| NF-9 | Structured logs to stdout, journald-friendly. Optional JSON format. |
| NF-10 | All configuration through `.env` / environment variables. No secrets in the repository or the database. |
