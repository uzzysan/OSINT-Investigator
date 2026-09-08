# 11: Sources, citations and unsourced-fact flagging

**What to build:** provenance. An analyst attaches a Source to an Entity, a
Relation, or a single Property value, so that a claim is recorded at the
granularity it was actually made at — the company registration came from the
registry extract, the phone number came from somewhere else, and the difference
matters when a conclusion is challenged.

Each Source records when it was retrieved, so a fact is dated to the moment it was
observed rather than the moment it was typed. URLs are normalised, so the same
page cited twice is one Source rather than two.

Facts without a Source are permitted, because capture has to stay fast. They are
flagged, and the analyst can ask which of their facts are unsourced — so the gaps
stay visible and can be closed before anyone else finds them.

Blocked by Relations because citing a Property value on a Relation requires
Relations to exist.

**Blocked by:** 10

**Status:** ready-for-agent

- [ ] Create Sources of each kind — link, file reference, manual note, agent run
- [ ] Each Source records a retrieval timestamp distinct from its creation time
- [ ] URLs are stored as given and in a normalised form
- [ ] A Source can be cited on an Entity, a Relation, or an individual Property value
- [ ] One Source can be cited many times; citations carry an optional note
- [ ] Reading an Entity or Relation returns its citations
- [ ] List every Source in a Case with what cites it
- [ ] Query which Entities, Relations and Property values have no Source
- [ ] Case-level counts of sourced versus unsourced facts are available
- [ ] Deleting a Source leaves the facts intact and marks them unsourced
- [ ] Every mutation is audited with its Actor, in the same transaction
