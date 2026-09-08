# Status Log

Newest entry first. Written before stopping, always, so the next session can
resume without archaeology.

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
  **`osint-core` pure functions** for normalisation, validation and schema
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

- Repository initialised at `C:\Kodzenie\1_Projekty\OSINT Investigator`, git
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
