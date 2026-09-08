# Glossary

Terms are used consistently across every document and in the code. Where a term
comes from an existing system, that lineage is noted.

| Term | Meaning |
| --- | --- |
| **Case** | An isolated investigation workspace. Owns entities, relations, sources, notes and a timeline. Roughly a Maltego "graph" or an Aleph "collection". |
| **Entity** | A node in the graph: a thing that exists in the world — a person, a company, a bank account, a car, a website. Has a schema and a bag of properties. |
| **Schema** | The *type* of an entity or relation, plus the properties it may carry. Named after FollowTheMoney's use of the word. `Person`, `Company`, `Ownership`. |
| **Property** | A named, typed, possibly repeated value on an entity or relation. `Person.birthDate`, `Company.taxNumber`. |
| **Property type** | The value domain of a property: `name`, `email`, `phone`, `date`, `country`, `identifier`, `url`, `text`, `number`, `entity`. Drives normalisation, validation and matching weight. |
| **Relation** | A typed, directed edge between two entities, itself carrying properties and an optional validity interval. |
| **Interval** | The `startDate`/`endDate` pair on a relation or event, with a stated precision. |
| **Source** | Where a fact came from: a URL, an uploaded file, a manual note, or an agent run. Carries a capture timestamp. |
| **Attachment** | A binary blob (photo, scan, PDF, screenshot) stored content-addressed and linked to a source. |
| **Observation** | A dated statement about an entity or relation. Feeds the timeline. |
| **Match candidate** | A machine-proposed pair of entities that may be the same real-world thing, with a score and an explanation. Never applied automatically. |
| **Same-as link** | A confirmed, *cross-case* assertion that two entities are the same thing. The entities stay separate; the link makes the bridge traversable. |
| **Merge** | A confirmed, *within-case* consolidation of two entities into one, with full undo. |
| **Blocking key** | A cheap, indexable string used to shortlist match candidates before expensive scoring. |
| **Canvas** | The interactive graph view. |
| **View** | A saved subset of a case's graph: filters, layout and camera position. |
| **Solo mode** | Deployment profile with no login, intended for one analyst on a trusted host. The default. |
| **Actor** | Whoever caused a change: `human`, or a named agent. Recorded on every write. |
