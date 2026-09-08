# 03: SQLite store, migrations and the single-writer runtime

**What to build:** the application opens its database, brings the schema up to
date, and is ready to serve. An operator who kills the process mid-migration and
restarts finds either the old schema or the new one, never a half-applied hybrid.

This ticket lays the full storage schema from the data model document as numbered
migrations, and establishes the concurrency model everything else will use: one
writer task fed by a channel, plus a pool of readers. Lock contention becomes
structurally impossible rather than merely unlikely, which is why no retry logic
appears anywhere in this codebase.

It also delivers the test harness the rest of the milestone depends on — a helper
that stands up a store against a fresh database in a temporary directory and
tears it down after. Later tickets should not each invent their own.

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] All tables, indexes and triggers from the data model document exist as numbered migrations
- [ ] Migrations run at startup inside a transaction; a failure leaves the previous schema intact
- [ ] Re-running against an up-to-date database is a no-op
- [ ] WAL, foreign keys, synchronous and busy-timeout pragmas are set on every connection
- [ ] Foreign key enforcement is proven by a test that expects a violation to be rejected
- [ ] A single writer task serialises all writes; a reader pool serves reads
- [ ] A test harness creates an isolated store in a temporary directory and cleans up after itself
- [ ] Repository traits are defined with a SQLite implementation behind them
- [ ] `osintd migrate` applies migrations and exits with an accurate status code
