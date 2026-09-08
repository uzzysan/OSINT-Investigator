# ADR-0002 — FollowTheMoney-inspired ontology with user extensions

**Status**: Accepted, 2026-09-08

## Context

Entity typing sits between two failure modes. Too loose (free-form nodes with
arbitrary attributes, the Maltego-lite approach) and the resolver has nothing to
reason about, the UI cannot render sensibly, and no two cases model a company
the same way. Too strict (implement all of FollowTheMoney's ~70 schemata and 21
property types faithfully) and the first release spends its budget on a
taxonomy, while users hit "there is no schema for what I found" and give up.

OCCRP's [FollowTheMoney](https://followthemoney.tech) is the only widely-used
open ontology built specifically for investigative data. It is battle-tested
across Aleph and OpenSanctions, and being compatible with it means data can move
in and out of a real ecosystem.

## Decision

Ship a curated subset of FollowTheMoney — about 30 entity schemas and 15
relation schemas — as built-in, embedded YAML, using FtM's schema ids, property
names and property types wherever they exist. Allow deployments to define custom
schemas and to add properties to built-in ones, stored in the database and
layered over the built-ins at startup.

Keep the property **type** system close to FtM's, because that is what drives
normalisation and match weighting, and it is the part that is genuinely hard to
get right from scratch.

Export in FtM JSON-lines so data can be pushed into Aleph or cross-referenced
against OpenSanctions.

## Alternatives considered

**Full FollowTheMoney.** Maximum interoperability and rigour. Rejected for v1 on
scope: 70 schemata is a lot of UI, a lot of icons, a lot of validation, and most
of it is unused by the target user. The subset is chosen so that growing toward
the full set is additive, never a rewrite.

**Fully user-defined schemas.** Most flexible and the least work upfront.
Rejected because the resolver depends on knowing that `Person.phone` is a phone
and `Company.taxNumber` is a checksummable identifier. Without built-in types
there is no sensible default matching behaviour, and agents have no shared
vocabulary to write against — every deployment would need its own prompt.

## Consequences

**Accepted**
- Schema ids and property names are a public contract from day one; renaming one
  is a migration.
- Custom schemas need a UI, validation, and a story for what happens on export
  (they map to FtM's nearest ancestor with extras preserved in `extra`).
- We will occasionally disagree with an FtM modelling choice and will have to
  either follow it for compatibility or document the divergence.

**Gained**
- Sensible defaults on day one: an agent that knows FtM already knows this app.
- Entity resolution has typed properties to weight.
- A path into Aleph and OpenSanctions without writing a translator later.
- Growing the ontology is data, not code.
