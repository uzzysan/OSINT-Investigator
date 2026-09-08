# 13: Bounded graph traversal

**What to build:** the queries that make this a graph rather than a list. An
analyst asks for an Entity's neighbours to a stated depth and explores outward
from something they know. They ask how two Entities are connected and get the
shortest path. They ask which parts of a Case are actually joined and which are
still isolated.

Every traversal is bounded by both a depth limit and a node budget, and a
truncated result is labelled as truncated. This matters more than it sounds:
expanding a hub in a real investigation can reach most of the database, and an
answer that is silently partial is worse than no answer — it reads as complete.

There is no unbounded traversal function, not even an internal one, because an
internal one becomes an external one.

**Blocked by:** 10

**Status:** ready-for-agent

- [ ] Fetch an Entity's neighbours to a given depth, with the connecting Relations
- [ ] Every traversal takes both a depth limit and a node budget
- [ ] A result that hit either limit is marked truncated
- [ ] No traversal path exists that omits the bounds
- [ ] Shortest path between two Entities, returning the Entities and Relations along it
- [ ] An absent path is reported as absent, not as an error
- [ ] Connected components within a Case
- [ ] Traversal can be filtered by Relation Schema and by Entity Schema
- [ ] Soft-deleted Entities and Relations are never traversed
- [ ] Cycles terminate correctly and each Entity is returned once, at its minimum depth
- [ ] Behaviour is verified against a seeded graph including a hub, a chain and an isolated component
