# Contributing

## Before you start

1. Read [`PLAN.md`](../PLAN.md) — it says what is done and what is next.
2. Read [`STATUS.md`](../STATUS.md) — it says where the last session stopped.
3. Read [`docs/wayfinder/map.md`](wayfinder/map.md) — it lists decisions that are
   still open. Building past an open decision is how rework happens.

## Working agreements

- **Back up before rewriting.** Any script that edits existing files copies them
  to `.backups/` first.
- **Commit per meaningful change**, with a short factual message describing what
  changed and why. Not one commit per session.
- **Update `PLAN.md` in the same commit** as the work it describes, and write
  `STATUS.md` before stopping.
- **Secrets live in `.env`**, which is git-ignored. Keep `.env.example` current
  with every new key, using placeholders. `gitleaks` runs in CI.
- **Tests accompany the code.** `vesica-core` and `vesica-resolve` are pure
  functions with no I/O and therefore no excuse.
- **A milestone is done when its exit condition is demonstrated**, not when its
  checkboxes look full.

## Commands

```bash
just dev          # backend + vite dev server with hot reload
just build        # release build with embedded web assets
just test         # workspace tests + web tests
just lint         # clippy -D warnings, eslint, tsc --noEmit
just fmt          # rustfmt + prettier
just audit        # cargo deny, cargo audit, npm audit, gitleaks
```

## Code layout

See [`docs/03-architecture.md`](03-architecture.md#33-cargo-workspace). The
dependency direction is one-way and enforced: `vesica-core` depends on nothing
internal and performs no I/O. If a domain rule needs a database to test, it is
in the wrong crate.

## Documentation

Design documents are numbered and live in `docs/`. Decisions that would be
expensive to reverse get an ADR in `docs/adr/`, using the existing format:
context, decision, alternatives seriously considered, consequences accepted.
Writing the alternatives section honestly is the point of the exercise.
