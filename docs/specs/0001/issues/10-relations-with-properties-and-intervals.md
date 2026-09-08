# 10: Relations with properties and intervals

**What to build:** the edges. An analyst connects two Entities with a typed,
directed Relation, so that "owns" and "is owned by" are recognisably the same
edge read from two ends.

Relations carry Properties of their own, so a shareholding records its
percentage rather than that living in a note. They carry a validity interval with
a precision, so a directorship that ended in 2021 is not displayed as current —
and a Relation known only to have existed "in 2019" says exactly that.

Schema endpoint constraints guide rather than forbid. The system knows which
combinations are sensible and steers toward them, but an explicit override is
allowed and flagged, because investigations routinely encounter things the
ontology did not anticipate and a tool that refuses them is a tool people work
around.

Deleting an Entity takes its Relations with it. The graph never contains an edge
to nothing.

**Blocked by:** 09

**Status:** ready-for-agent

- [ ] Create a typed, directed Relation between two Entities in the same Case
- [ ] Relations carry their own Properties, normalised on the same code path as Entity Properties
- [ ] A validity interval with an explicit precision can be set, and partial dates are honoured
- [ ] Endpoint Schema constraints are checked; a violation is permitted with a recorded warning
- [ ] Undirected Relations are supported and distinguishable from directed ones
- [ ] Read a Relation; list Relations by Case, by Schema, and by either endpoint
- [ ] Update and soft-delete Relations
- [ ] Deleting an Entity cascades to its Relations, and restoring the Entity restores them
- [ ] Relations across Cases are rejected with an explanatory error
- [ ] Every mutation is audited with its Actor, in the same transaction
