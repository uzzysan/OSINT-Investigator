# Status Log

Newest entry first. Written before stopping, always, so the next session can
resume without archaeology.

---

## 2026-09-09 — M0 complete

**Done** — all seven M0 tickets, built by subagents under orchestration, each
verified independently before being committed.

- 01 workspace · 02 config, logging, CLI · 03 store, migrations, single-writer
  runtime · 04 health, readiness, embedded SPA · 05 CI and installer ·
  06 property types and normalisation · 07 schema registry.
- **160 tests pass.** `cargo fmt --check`, `cargo clippy -D warnings`,
  `just check-deps` and `cargo deny check` are all clean; `gitleaks` finds
  nothing, and was proven to work by planting a secret and watching it fail.
- The service was run by hand: `/api/v1/health` returns 200 without touching the
  database, `/api/v1/ready` returns 200 reporting migrations, database
  writability and blob-directory writability separately, the embedded SPA serves
  at `/`, a taken port produces a clear refusal instead of moving, and SIGTERM
  drains and closes cleanly.
- `deploy/install.sh` was executed for real (with `--no-service`, since the build
  container has no systemd): it built the web assets and the release binary,
  created the `vesica` system user and a 0750 data directory, ran migrations, and
  the resulting binary answered health and ready.

**Corrections to the design docs, all found by the implementation**

- The dependency direction did not account for the binary being the composition
  root, and omitted `vesica-core` from the api/mcp edges.
- The Operations table listed `/health` and `/ready` without the `/api/v1` prefix
  that section 6.1 and the architecture doc both require.
- The traversal CTE in section 4.4 bounded its result but not its work, so a
  cycle could run away regardless of the budget.
- The systemd unit still said `User=osint`, missed by the Vesica rename.
- The data model told both `iban` and `wallet` to uppercase; uppercasing a wallet
  address destroys base58 and discards the EIP-55 checksum.

**Known gaps carried forward**

- The systemd path could not be exercised to a running service — the build
  container has no systemd. Everything up to `systemctl daemon-reload` is
  written and the "systemd present but not PID 1" guard fails cleanly.
- The toolchain auto-install branches in `install.sh` are unexercised, since
  Rust and Node were already present.
- The database-writability readiness check has no negative test: an already-open
  file descriptor does not re-check permissions on Unix.
- `vesica-store` repositories are deliberately thin — no property read/write, no
  blob or match repos, no shortest path or connected components yet.

**Next action**

M1 proper: ticket 08 (case lifecycle), then 09 (entities), 10 (relations),
11 (sources), 12 (search), 13 (traversal), 14 (audit), 15 (custom schemas).
Ticket 004 in the wayfinder map — the full schema catalogue — should be settled
before 09 hardens the property contract.

---

## 2026-09-08 (design) — Design system

**Done**

- [`docs/10-design-system.md`](docs/10-design-system.md): principles, colour
  tokens for both themes with measured contrast, three-family type system and
  scale, spacing/radius/elevation, component rules, graph canvas rules, motion
  tokens, responsive and mobile, accessibility, implementation notes.
- A design canvas saved as an artifact with nine artboards across two pages:
  brand, colour, typography, components, motion; then the workspace at 1440×900
  in dark and light, and two mobile screens at 390×844.
- Direction: IBM Plex Sans/Mono with Instrument Serif for display; signal cyan +
  attention amber accents at one lightness and chroma; dark primary.

**Notes**

- Two alternate accent directions are drawn on the colour sheet in case cyan +
  amber does not land. Swapping is a token change.
- The wordmark is provisional pending the naming decision (ticket 003).

---

## 2026-09-08 (later still) — Spec 0001 broken into tickets

**Done**

- Fifteen tracer-bullet tickets under
  [`docs/specs/0001/issues/`](docs/specs/0001/issues/), numbered in dependency
  order with explicit blocking edges, each sized for one fresh context window and
  verifiable on its own. All labelled `ready-for-agent`.
- Two chains that run in parallel after ticket 01: deployment (02 → 03 → 04 → 05)
  and domain (06 → 07), meeting at 09 where Entities need both the schema
  registry and Cases.
- Deliberate edges recorded in the ticket index: 08 (Cases) is not blocked by the
  schema registry, since a Case needs no Schemas; 11 (Sources) is blocked by 10
  rather than 09, since citing a Property value on a Relation needs Relations.

**Next action**

Resolve tickets 002 and 003 in the wayfinder map (build loop, product name), then
take [ticket 01](docs/specs/0001/issues/01-cargo-workspace-and-crate-skeleton.md)
— the only one on the frontier at the start.

---

## 2026-09-08 (later) — Spec 0001 written

**Done**

- [Spec 0001 — Foundation and domain core](docs/specs/0001-foundation-and-domain-core.md)
  covering M0 + M1: problem statement, solution, 75 user stories, implementation
  decisions, testing decisions, out-of-scope and notes. Labelled
  `ready-for-agent`.
- Test seams chosen and recorded: **the service layer** as the primary seam
  (tested against a real temp SQLite file, and the same struct both axum and rmcp
  will adapt in Spec 0002, so one suite covers both transports), plus
  **`vesica-core` pure functions** for normalisation, validation and schema
  resolution. Repositories are deliberately *not* a seam.
- `docs/specs/` index created; PLAN.md, README.md and the wayfinder map now point
  at it.

**Notes**

- No issue tracker is configured for this repository, so specs live as markdown
  in `docs/specs/` with labels in front matter. Running
  `/setup-matt-pocock-skills` would wire up a real tracker if that is wanted.
- Spec 0001 depends on three still-open tickets and says so rather than assuming
  them closed: 002 (build loop), 003 (name), 004 (schema catalogue). The spec
  specifies a provisional starter schema set so 004 does not block a start.

**Next action**

Resolve tickets 002 and 003, then begin M0 against Spec 0001.

---

## 2026-09-08 — Design phase complete

**Done**

- Repository initialised at `C:\Kodzenie\1_Projekty\Vesica`, git
  repository created, `.gitignore` and `.env.example` written.
- Researched the space: OCCRP Aleph + FollowTheMoney as the ontology reference,
  Maltego as the interaction reference, OpenSanctions `nomenklatura` as the
  entity-resolution reference, `rmcp` as the MCP implementation.
- Four foundational decisions taken with the operator and recorded as ADRs:
  single Rust binary + SQLite; FtM-inspired ontology with user extensions; no
  built-in collectors (agents feed the graph); solo mode with an auth seam.
- Complete design documentation set written: glossary, vision and scope,
  functional specification (~90 numbered requirements), architecture, data model
  with full SQLite schema, entity-resolution algorithm, HTTP API, MCP server
  surface, deployment, and security/privacy/legal.
- `PLAN.md`: nine milestones (M0–M8), each with checkboxes and a demonstrable
  exit condition.
- Wayfinder map with seven open decision tickets.

**In flight**

Nothing. Clean stopping point.

**Environment notes for the next session**

- The project folder is reached through a Linux VM on `rafal-y540`. It has
  Node 22, Python 3.10, git and network access. It has **no Rust toolchain and
  no Docker**. `rustup` should be installable from there — untested.
- Each shell call into that VM has roughly a 45-second ceiling. Long builds must
  run detached with output to a log file that gets polled, or happen elsewhere.
  This is [ticket 002](docs/wayfinder/tickets/002-build-and-test-loop.md).
- No `.env` exists yet — only `.env.example`. Nothing to protect in the
  repository so far.

**Next action**

Resolve the frontier tickets before writing code, in this order:

1. [002 — Build and test loop](docs/wayfinder/tickets/002-build-and-test-loop.md)
   — shapes every session that follows.
2. [003 — Product and binary name](docs/wayfinder/tickets/003-product-name.md)
   — must be settled before M0 creates the crates.
3. [004 — Built-in schema catalogue](docs/wayfinder/tickets/004-schema-catalogue.md)
   — the largest piece of thinking left, and M1 waits on it.
4. [001 — Licence](docs/wayfinder/tickets/001-licence.md) — cheapest now,
   expensive after a public commit.

Then M0: the Cargo workspace, the binary skeleton, CI, and an `install.sh` that
gets an empty application onto a box.
