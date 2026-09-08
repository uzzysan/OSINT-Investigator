---
id: 004
title: Built-in schema catalogue
type: grilling
status: open
blocked_by: []
assignee:
---

## Question

Exactly which entity and relation schemas ship as built-ins, and what properties
does each carry?

[ADR-0002](../../adr/0002-ftm-inspired-ontology.md) settled the *approach* — a
curated FollowTheMoney subset, extensible. It did not settle the *list*.
[`docs/02-functional-spec.md`](../../02-functional-spec.md#built-in-schemas-v1)
sketches roughly 30 entity schemas and 15 relation schemas; that sketch has not
been reviewed against the investigations this will actually be used for.

**Why it needs deciding before M1.** Schema ids and property names are a public
contract: the resolver weights them, agents write against them, exports map them,
and renaming one later is a migration. Authoring the YAML is a day's work;
authoring the *wrong* YAML costs more than that.

**What to work through**

- Which of the sketched schemas earn their place, and which are speculative?
  (`Vessel` and `Airplane` are in FtM; are they in *these* investigations?)
- Which are missing? Polish-specific identifiers (NIP, REGON, KRS, PESEL) are
  handled as `identifier` properties, but is there a case for dedicated ones?
- Per schema: the property list, which are multi-valued, which are required, the
  caption template, and the match weight of each property.
- Which relation schemas need intervals, and which endpoint schema constraints
  are hard versus advisory?
- Do we ship a `Custom` escape hatch entity type for v1, so a user is never
  blocked while waiting for a schema?

**Output**: the concrete YAML content of `crates/osint-core/schemas/`.

## Answer

_(unresolved)_
