# 08: Case lifecycle

**What to build:** an analyst can create a Case, work in it, archive it when the
investigation concludes, and delete it without that being an irreversible
decision. Every Case has a slug so it can be referenced stably later, and a
default region so that local phone numbers entered inside it normalise correctly.

Deleting is soft and reversible. Purging is a separate action that must be
explicitly confirmed, and actually destroys the data — because when data needs to
be gone, "mostly gone" is not an answer.

This is the first ticket to build the service layer, so it establishes its shape:
a struct constructed from configuration and a store handle, exposing one method
per use case, taking and returning domain types. Every business rule in this
project lives here. It also establishes the fixture pattern that the remaining
tickets build their tests on.

Deliberately not blocked by the schema registry: a Case needs no Schemas. The
Services constructor gains a registry field in the next ticket.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] The service layer struct exists, constructed from configuration and a store handle
- [ ] A test fixture builds a working service against a temporary database in one call
- [ ] Create, read, list and update a Case
- [ ] Slugs are unique, derived from the name, and stable across renames
- [ ] A default region can be set and is retrievable for later normalisation
- [ ] Archiving a Case removes it from the active list without destroying it
- [ ] Soft deletion is reversible; deleted Cases are excluded from every read
- [ ] Purge is a separate operation, refused unless explicitly confirmed, and removes the data
- [ ] Every mutation writes an audit entry recording the Actor, in the same transaction
- [ ] The Actor defaults to the analyst and can be overridden by a caller
