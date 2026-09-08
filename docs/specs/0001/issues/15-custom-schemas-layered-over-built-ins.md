# 15: Custom schemas layered over built-ins

**What to build:** a deployment owner hits something the built-in ontology does
not cover and it is not a dead end. They define a new entity type with its own
Properties, or add Properties to a built-in Schema, and start recording — without
forking the project and without waiting for an upstream release.

Custom definitions are stored in the database and layered over the embedded
built-ins at startup. A custom Schema can extend a built-in one, inheriting its
Properties, so extending is the normal case and replacing is the exception.

A definition that conflicts with a built-in is rejected at write time with an
error explaining the conflict — not accepted and then discovered broken at the
next restart, which would turn a typo into an outage.

**Blocked by:** 09

**Status:** ready-for-agent

- [ ] Define a custom entity Schema with its own Properties and have it usable immediately
- [ ] Define a custom relation Schema with endpoint constraints
- [ ] Add Properties to a built-in Schema without replacing it
- [ ] A custom Schema can extend a built-in one and inherits its Properties
- [ ] Custom definitions are validated on write with the same rules as built-ins
- [ ] A conflicting definition is rejected at write time with an error naming the conflict
- [ ] Custom Schemas layer over built-ins at startup and appear in the full catalogue
- [ ] Entities using a custom Schema behave identically to those using a built-in one
- [ ] A custom Schema that is in use cannot be deleted; the error says how many Entities depend on it
- [ ] Removing an unused custom Schema restores the built-in behaviour it was overlaying
- [ ] Schema definition changes are audited
