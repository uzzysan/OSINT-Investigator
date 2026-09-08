# ADR-0001 — Single Rust binary with embedded SQLite

**Status**: Accepted, 2026-09-08

## Context

The deployment requirement is explicit and unusually load-bearing: clone a
repository onto a dedicated machine, run one script, get a service on a free
port that a human or an agent can reach. Every component that must be separately
installed, configured, secured, monitored, upgraded and backed up makes that
requirement harder to meet and easier to break.

The workload is one analyst plus a handful of agents against a graph that is
large by human standards and small by database standards: tens of thousands of
entities per case, low hundreds of thousands overall, a few writes per second at
peak, mostly reads.

## Decision

One Rust binary (`vesica`) containing the HTTP API, the web UI assets, the MCP
server and a background worker, backed by one SQLite database file in WAL mode
and one content-addressed blob directory.

- Traversal via recursive CTEs, bounded by depth and node budget.
- Full-text via FTS5; fuzzy matching via a trigram pass over the FTS shortlist.
- Web assets embedded with `rust-embed`.
- One writer task, a reader pool.
- Persistence behind repository traits in `vesica-store`, so a second backend is
  possible without touching the rest of the codebase.

## Alternatives considered

**Docker Compose with PostgreSQL.** Better concurrent writes, `pg_trgm` for
fuzzy matching, and a clear scaling path. Rejected because it makes the
installer responsible for a container runtime, volumes, networking and a second
daemon's lifecycle, and turns backup from `cp` into a procedure. The write
concurrency it buys is not needed at this scale.

**Docker Compose with Neo4j.** The best traversal engine here by a distance, and
Cypher is genuinely nicer than recursive CTEs for path queries. Rejected on
operational weight: a JVM using ~2 GB at idle, licensing questions for the
non-community features, and a second store still required for documents and
blobs. The graph queries this application actually runs — neighbourhood
expansion to depth 3, shortest path, connected components — are comfortably
expressible in SQL at our size.

**Embedded graph database (KùzuDB, Oxigraph).** Attractive on paper: embedded
*and* graph-native. Rejected as too young for a system that holds data people
cannot easily reconstruct. SQLite's durability record and tooling are worth more
here than query elegance.

## Consequences

**Accepted**
- One writer. Concurrent write throughput is capped. At the target scale this is
  invisible; if it ever is not, the repository seam is where Postgres goes.
- Recursive CTEs are more verbose than Cypher, and deep unbounded traversal
  is not viable — so every traversal is explicitly bounded, which is a
  discipline we want anyway.
- Graph analytics beyond degree and components run client-side in `graphology`
  over the loaded subgraph, not server-side over everything.
- No horizontal scaling. Not a goal.

**Gained**
- `git clone && ./deploy/install.sh` genuinely works.
- Backup and restore are file operations.
- Runs on a Raspberry Pi, and runs air-gapped.
- Idle footprint under 150 MB, so it can share a box with other things.
- No version-skew between the app and its database server.
