# 14: Audit history and integrity check

**What to build:** the ability to answer "how did this get here?" An analyst opens
an Entity and sees its full change history — what was asserted, by whom, and when.
The distinction between what they asserted themselves and what an agent asserted
is visible at a glance, which is what makes agent-written data trustworthy enough
to keep.

Related changes are grouped under one batch, so an import or a merge can be
understood as a single act rather than as forty unrelated edits. Later milestones
undo by batch; this ticket makes the grouping real.

The log is append-only by having no update or delete code path at all. Not a
convention, an absence of capability. If a future requirement seems to need one,
that is a signal to write a compensating entry instead.

An operator also gets a way to check the database is sound without reading SQL.

**Blocked by:** 11

**Status:** ready-for-agent

- [ ] Every mutation across Cases, Entities, Relations and Sources is recorded with its Actor and timestamp
- [ ] Audit entries capture enough before-and-after state to reconstruct what changed
- [ ] Read an Entity's full history in order
- [ ] Read a Case's recent activity
- [ ] Filter the log by Actor, target, batch and time range
- [ ] Related mutations share a batch identifier; a caller can open a batch and write under it
- [ ] The codebase contains no path that updates or deletes an audit entry
- [ ] Audit writes are in the same transaction as the change, proven by a test where a rolled-back write leaves no entry
- [ ] `osintd check` verifies foreign key integrity, finds orphaned rows, and confirms every Entity's Schema exists in the registry
- [ ] The check reports findings in plain language and exits with an accurate status code
