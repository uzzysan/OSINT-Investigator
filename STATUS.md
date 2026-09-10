# Status Log

Newest entry first. Written before stopping, always, so the next session can
resume without archaeology.

---

## 2026-09-10 (later) — Tickets 09 and 10 done; the repo could not be committed to from the cloud

**Done**

- **Ticket 09 — entities, properties and tags.** The act the application exists
  for. Create an Entity from a Schema and a set of repeating Properties; read it
  with its values and tags; list within a Case filtered by schema, confidence or
  tag; change caption and confidence; add, replace and remove Property values;
  tag and untag; soft delete, restore, and a purge that must be confirmed. Each
  value keeps the raw input beside its normalised form, normalised in the Case's
  default region. Checksum failures surface as warnings on the returned Entity
  rather than refusing the value. Captions derive from the Schema template and
  can be overridden. Caller-supplied ULIDs are accepted so an agent can build a
  subgraph locally and submit it whole; a duplicate is refused with a usable
  message. A merged-away id still resolves, reporting the survivor.
  `repo::entity` became an executor-generic function module covering
  `entity_prop`; `repo::tag` is new.
- **Ticket 10 — relations with properties and intervals.** Typed, directed edges
  within one Case, carrying their own Properties and a validity interval with an
  explicit precision, so a directorship that ended in 2021 is not drawn as
  current and an edge known only to have existed "in 2019" says exactly that.
  Endpoint Schema constraints guide rather than forbid: a violation is recorded
  and flagged, not refused. Undirected relations are distinguishable from
  directed ones and answer an endpoint query from either end. Cross-case
  relations are refused with an explanation. `repo::relation` converted the same
  way; the ticket-03 `RelationRepo` trait is gone and `source` is now the last
  surviving repository trait.
- **The property pipeline is shared, not duplicated.** Ticket 10 generalised
  ticket 09's `check_shape` / `require_required` / `normalize_all` / `plan_edit`
  over a `PropertyHost` trait implemented by both `EntitySchema` and
  `RelationSchema`. There is one multiplicity rule, one required-property rule
  and one normalisation call for entity and relation values alike.
- **Cascade and undo.** Deleting an Entity takes its Relations with it, under a
  `batch_id` shared with the Entity's own delete entry, and restoring the Entity
  restores exactly that batch. A Relation the analyst deleted on its own
  beforehand belongs to a different batch and stays deleted. The alternative —
  matching on `deleted_at` — breaks whenever two deletes land in the same
  millisecond, which the test suite reaches easily.

**Verified independently, not taken on report**: 299 tests pass (213 at the
start of the session); `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `just check-deps` and `cargo deny check` all
clean. Every gate was run by the orchestrating session after each ticket, not
only by the agent that wrote it.

**Two defects found in review of ticket 09, both fixed before it was accepted**

- **A no-op check that could never fire.** `update_entity` decided "did anything
  change" by comparing the assembled `Entity` before and after — but it had
  already set `updated_at` to the current time, and `Entity` compares
  `updated_at`. So the two were never equal unless the update landed in the same
  millisecond as the create, and every no-op update wrote an audit entry saying
  nothing had happened. Now the comparison is made on the fields the use case can
  actually change, before the timestamp is touched. Worth noting that this is the
  second timing-dependent bug in two sessions (the first was the ULID one below):
  both were checks that silently degraded to no-ops rather than failing loudly.
- **Two tests asserting normalisation `vesica-core` deliberately does not do.**
  They expected an email's local-part dots to be folded for any domain (that is a
  Google-only rule, and applying it everywhere merges two different people at any
  provider where a dot is significant) and expected `value_norm` to transliterate
  `Łukasz` to `lukasz` (`ł` has no NFKD decomposition; the ASCII transcription is
  a blocking key, which belongs to the resolver in M5 and is not stored on the
  Property). The implementation was right and the tests were wrong. Both now
  assert the real behaviour, with the reasoning written down, and each gained a
  second case pinning the other half of the rule.

**A decision that changed a standing rule, recorded here because it did**

`docs/AGENT-BRIEF.md` said there was exactly ONE update path against
`audit_log`. There are now two: `redact_case_payloads` for a Case purge and
`redact_target_payloads` for an Entity purge. They are separate because
`docs/09-security-and-legal.md` owes erasure to a purged Entity as well as a
purged Case, and reusing the case-scoped function for one Entity would erase the
audit payloads of every other Entity in that Case — destroying history nobody
asked to erase in the name of erasing history somebody did. AGENT-BRIEF.md and
the service-layer module docs were updated to say two, and to say there must
never be a third.

**Not done — and the reason matters for how the next session should be set up**

- **Nothing could be committed into the project folder from the cloud session.**
  Two independent blocks: the device shell never mounted the folder for the whole
  session (`sandbox-helper: no Plan9 drive shares mounted under
  /mnt/.virtiofs-root/shared`), so there was no `git` on the Windows machine; and
  the remote file tools refuse every path under `.git` outright — *"Writing to
  .git is not permitted via remote tools."* Loose objects, refs and the index
  were all rejected.
- **The work is therefore delivered as a git bundle**, `Claude
  outputs/vesica-tickets.bundle`, with `Claude outputs/HOW-TO-APPLY.md` beside
  it. This is better than syncing loose files would have been: a bundle keeps
  each ticket as its own commit with its own message, where loose files would
  have collapsed both into one undifferentiated pile and left the tree dirty.
  It was verified by applying it to a throwaway clone, which produced a tree
  identical to the one the tests ran against. **Until somebody runs `git pull
  "Claude outputs\vesica-tickets.bundle" master`, the working tree here is still
  at `c0dd677` and none of this work is in it.**
- If the next session wants to commit directly again, the device shell's mount
  has to be working. That is worth checking at the start of a session rather
  than discovering after a ticket is finished.

**Known gaps carried forward from these two tickets**

- **No `purge_relation`.** Purging an Entity or a Case destroys relations through
  the foreign-key cascade, but writes no per-relation audit entry and redacts no
  relation audit payload — and a relation's payload names both its endpoints. If
  the erasure obligation reaches a purged Entity's edges, that is unfinished.
- **`relation_prop` has no date columns.** No relation Schema declares a `date`
  Property today, so nothing is broken; if one ever does, its value is stored but
  `PropertyValue::date` comes back `None`. A trap for whoever adds such a Schema.
- **Endpoint warnings are derived at read, not stored.** Deliberate: the warning
  is today's ontology judging data that has not changed, so when the catalogue
  grows a Schema that makes the combination sensible the warning must stop
  appearing without a migration. What is recorded permanently is the audit
  snapshot naming the flagged endpoints' schema ids.
- **The cascade is bounded at 1,000 passes of 1,000 rows** and returns
  `Inconsistent` beyond that rather than leaving a half-cascaded graph.
- **Spec drift, flagged but not fixed** (agents may not edit `docs/`):
  `docs/specs/0001-foundation-and-domain-core.md` still describes repositories as
  traits and says compile-time-checked queries are used, both superseded; and
  `docs/04-data-model.md` 4.1.3 shows `Ownership`'s source as `Organization`,
  which is not in the catalogue. None of it blocked the work.

**Next action**

Apply the bundle. Then the frontier is unusually wide: **11 (sources), 12
(search), 13 (traversal) and 15 (custom schemas) are all unblocked and
independent of each other** — the first real opportunity to run tickets in
parallel, provided each agent gets its own `CARGO_TARGET_DIR` and the file
ownership is checked first (11, 12 and 15 all touch the service layer, so their
disjointness is not obvious and should be established before, not during).
14 stays blocked behind 11.

Wayfinder ticket 004 — the full built-in schema catalogue — is now more pressing
than it was: 09 and 10 both hardened the property and endpoint contracts against
a schema set that is still explicitly provisional.

---

## 2026-09-10 — Ticket 08 done; a latent ordering bug found and fixed

**Done**

- **Ticket 08 — case lifecycle.** The service layer (`vesica_api::services::Services`)
  is established: one method per use case, taking and returning domain types, with
  `vesica_api::testing::test_services()` as the fixture every later ticket's tests
  use. Cases create/read/list/update, slugs stable across renames, default region,
  archive, soft delete and restore, and a purge that is refused without explicit
  confirmation. Every mutation writes its audit entry in the same transaction.
- **Erasure by redaction.** A purge now strips the payloads from the purged case's
  earlier audit rows, leaving the rows themselves — actor, action, timestamp —
  intact, per the obligation in `docs/09-security-and-legal.md`. There is exactly
  one update path against `audit_log`, documented as the sole exception to the
  append-only rule. Scoped by `case_id` rather than `target_id`, so it already
  covers the entities a purge cascades through.
- **A real bug: ULIDs were not monotonic.** `Ulid::new()` puts a millisecond
  timestamp in the high bits and *random* bits in the low 80, so ids minted in the
  same millisecond sorted arbitrarily. Every list orders by
  `created_at DESC, id DESC` and leans on `id` to break the tie — so under a burst
  (an agent batch, an import) the sort was unstable, and an unstable sort under
  pagination means a row can appear on two consecutive pages or on neither. Fixed
  with `ulid::Generator`, which increments the previous random component within a
  millisecond. A regression test mints 500 ids in a tight loop and asserts they are
  strictly increasing.

  Worth recording *how* this was found: the test that caught it had been passing.
  It only failed when the workspace was rebuilt on a different machine, where the
  timing differed. It would otherwise have shipped.

**Verified**: 213 tests pass; `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `just check-deps` and `cargo deny check` all clean.

**New**: `docs/AGENT-BRIEF.md` — the standing brief for implementation agents.
It previously lived only in the orchestrating session and was lost when that
session's container was reclaimed. It is now part of the repository.

**Not done**

- **Ticket 09 (entities, properties, tags) was started and lost.** The session's
  cloud container was reclaimed mid-ticket. Nothing else was affected, because
  each finished ticket had already been synced and committed here. Ticket 09 needs
  starting from scratch.

**Next action**

M1, working the frontier: ticket 09 (entities), then 10 (relations), then
{11 sources, 12 search, 13 traversal, 15 custom schemas} which are independent of
each other, then 14 (audit history and integrity check).

Wayfinder ticket 004 — the full built-in schema catalogue — is worth settling
before or alongside 09, since 09 hardens the property contract. It is a data
change, not a code change, so it does not hard-block.

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
