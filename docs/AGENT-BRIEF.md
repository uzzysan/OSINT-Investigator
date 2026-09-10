# Standing brief — every Vesica implementation agent reads this first

You are implementing **one ticket** of Vesica: a self-hosted, case-based
investigative knowledge graph with cross-case entity resolution and an MCP
server. You are not designing it — the design is settled and written down. Build
your ticket exactly, test it, and report.

## Where things are

| Path | What |
| --- | --- |
| repository root | The workspace. A Cargo workspace plus `web/`, `migrations/`, `deploy/`. |
| `docs/specs/0001-foundation-and-domain-core.md` | The spec your ticket belongs to. |
| `docs/specs/0001/issues/NN-*.md` | Your ticket. Read it in full. |
| `docs/03-architecture.md` | Crate layout, dependency direction, request flow. |
| `docs/04-data-model.md` | The ontology and the complete SQLite schema. |
| `docs/06-http-api.md` | REST surface. |
| `docs/08-deployment.md` | Installer, systemd unit, ports, backups. |
| `docs/09-security-and-legal.md` | Threat model, secrets, data-protection obligations. **Read before implementing anything destructive or anything that stores personal data** — it settles at least one question per destructive operation. |
| `docs/10-design-system.md` | Tokens, type, components, motion. For UI work. |
| `.env.example` | Every configuration key, with its meaning. |

## Naming — settled, do not vary

Product **Vesica**. Crates `vesica-core`, `vesica-store`, `vesica-resolve`,
`vesica-api`, `vesica-mcp`, `vesica-report`, `vesica-import`, binary `vesica`.
Service `vesica.service`, install path `/opt/vesica`, data `/var/lib/vesica`,
database `vesica.sqlite`, environment prefix `VESICA_`, MCP server id `vesica`.

Keep the name out of string literals wherever a constant will do.

## Hard rules

1. **Do only your ticket.** Don't implement the next one because it looks easy,
   and don't refactor another ticket's code. If something you need is missing,
   build the smallest stub that unblocks you and say so in your report.
2. **The dependency direction is one-way and enforced** by `just check-deps`.
   `vesica-core` depends on no other workspace crate and performs no I/O — no
   `sqlx`, `tokio`, `axum`, filesystem or network. **`vesica-resolve` is
   I/O-free for the same reason**: it owns blocking keys and the scorer as pure
   functions; the worker that drains the `match_job` queue lives in the service
   layer. The binary is the composition root and is deliberately exempt.
3. **`#![forbid(unsafe_code)]` in every crate.**
4. **No string-built SQL, ever** — including dynamic filters. Bound parameters
   only.
5. **Every traversal and every list is bounded**: a depth limit and a row budget,
   and a flag saying whether the result was truncated. No unbounded variant, not
   even a private one.
6. **Tests ship with the code.** A ticket with no tests is not done.
7. **`cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D
   warnings` must both pass.** Not "mostly".
8. **Never write a secret anywhere.** Config comes from the environment and
   `.env`. `.env` is git-ignored; `.env.example` gains an entry for every new key.
9. **Do not `git commit`.** The orchestrator reviews and commits.
10. **Do not edit files under `docs/`.** If a document is wrong, say so in your
    report — do not fix it yourself.

## Toolchain and dependency policy

Rust 1.95, **edition 2024** (note: `gen` is a reserved keyword), MSRV 1.90. All
shared dependencies are declared once in `[workspace.dependencies]` and
referenced as `dep.workspace = true`. Never pin a different version in a member
crate.

Adding a dependency is a decision, not a convenience: this tool holds other
people's investigations, and a supply-chain compromise reads them. If you must
add one, say what it is, why nothing present does the job, and what it pulls in.

**sqlx is used with runtime-checked queries** (`sqlx::query`, `query_as`,
`query_scalar`) — NOT the compile-time macros. Deliberate, recorded so nobody
"fixes" it: the macros need a live `DATABASE_URL` or committed offline metadata
at build time, making every downstream build depend on a database fixture.
Runtime-checked queries keep what matters — everything parameterised, nothing
concatenated — and correctness is proven by integration tests. `sqlx`'s `derive`
feature IS enabled for `FromRow`; that is not the macro family in question.

## The build loop

Two cores, 7 GB RAM. A cold workspace build takes 15–25 minutes. Run long builds
detached and poll, and **never let a foreground call time out — it kills the
background job**:

```bash
setsid nohup cargo test --workspace > /tmp/t.log 2>&1 < /dev/null &
disown
# poll: grep -E "^test result" /tmp/t.log
```

Don't pipe the build through `tail` — the pipeline buffers and the log stays
empty until the end. `cargo check` is much faster than `cargo build` while
iterating.

## Style

- Errors: `thiserror` in libraries, `anyhow` only in the binary. Every validation
  error carries a machine-actionable `fix` string saying what a valid value looks
  like — an agent reads it to correct itself without a human.
- Ids are ULIDs as 26-char strings. `vesica_store::ids::new_id()` is **monotonic
  within a millisecond** and must stay that way — every list orders by
  `created_at DESC, id DESC` and relies on `id` to break the tie. A plain
  `Ulid::new()` randomises the low bits and breaks pagination under bursts.
- Timestamps are ISO-8601 UTC strings at the storage boundary.
- Prefer explicit over clever. Someone will read this at 2am trying to work out
  why a match was proposed.

## Current state — M0 complete, M1 started

Everything below exists and works. **Read the code before rebuilding any of it.**

- **vesica-core** — the pure value layer: 16 `PropertyType` variants each with
  parse-and-validate, normalise (returning warnings — a failed checksum is a
  WARNING and the value is still stored) and blocking-key derivation;
  `PartialDate` with no accessor that yields an instant; hand-rolled
  IBAN/VIN/NIP/PESEL/REGON checksums. Plus the schema registry:
  `SchemaRegistry::builtin()`, 11 entity and 6 relation schemas as embedded
  YAML, inheritance resolved, validated at load, golden-tested.
- **vesica-store** — migrations 0001–0006 (all 20 tables plus `entity_fts` and
  its sync triggers), applied by `Db::open` in one transaction. Pragmas on every
  connection. A single writer task (`Db::write`) plus a reader pool
  (`Db::read_pool`), and `Db::transaction` for atomic multi-step writes.
  Repositories are **executor-generic function modules, not traits** — ticket 08
  converted them so a change and its audit row share one transaction. Bounded
  `graph::neighbors`. **`vesica_store::testing::test_store()`** for store tests.
- **vesica-api** — axum router, `/api/v1/health`, `/api/v1/ready`, one auth
  middleware over every route, embedded SPA, graceful shutdown. **And the
  service layer**: `services::Services` — one method per use case, taking and
  returning domain types. No business rule may live in an axum handler or an MCP
  tool; both are thin adapters over this struct, which is what lets one test
  suite prove behaviour for both transports.
  **`vesica_api::testing::test_services()`** is the fixture every service test
  uses; add seeding helpers there, not per test file.
- **vesica** — config from env/.env with validation and redacted secrets,
  structured logging, the full CLI. `migrate` and `serve` implemented.
- **Deployment** — CI workflow, `deny.toml`, `deploy/install.sh` (executed for
  real), hardened systemd unit.

**213 tests pass.** fmt, clippy, `just check-deps` and `cargo deny check` are all
clean. Breaking an existing test is a defect in your ticket, not an inconvenience.

**On the ontology**: the 11+6 schema set is PROVISIONAL. The full catalogue is
still an open decision (wayfinder ticket 004). Never hardcode a schema id or
property name in logic — drive everything off the registry.

**On erasure**: `purge` redacts the payloads of the target's earlier audit rows
rather than deleting them, per `docs/09-security-and-legal.md`. There are exactly
TWO update paths against `audit_log` in the codebase and they are the only
exceptions to its append-only rule: `repo::audit::redact_case_payloads`, scoped
by `case_id`, for a Case purge, and `repo::audit::redact_target_payloads`, scoped
to one row, for an Entity purge (ticket 0001-09). They are separate because
`09-security-and-legal.md` owes erasure to a purged Entity as well as a purged
Case, and reusing the case-scoped one for a single Entity would erase the
payloads of every other Entity in that Case as well. Both are documented at
length where they are defined. Do not add a third, and do not reuse either for
anything that is not an erasure.

## Your report

End with a structured summary. It is the only thing the orchestrator sees:

1. **What you built** — in behaviour terms, not a file listing.
2. **Acceptance criteria** — each one from your ticket, with met / not met / n/a
   and a one-line justification. Never claim `met` for something you did not run.
3. **Verification** — the exact commands and their outcome. Paste the final lines
   of the test output.
4. **Files added or changed** — paths only.
5. **Decisions you had to make** that the ticket did not settle, and why.
6. **Problems, stubs and anything left behind** — including anything you believe
   is wrong in the specs or in this brief. Be blunt; an overstated report costs
   more than a problem flagged early.
