# 07: Schema registry — the ontology as data

**What to build:** the vocabulary an analyst records findings in. Built-in Schemas
for the things investigations actually contain — people, companies, addresses,
accounts, vehicles, websites, documents — each declaring its Properties with
types, so the system knows a phone number is a phone number.

Schemas are YAML embedded in the binary, not Rust types. Adding an entity type is
a data change, not a migration. Inheritance means everything that is a legal
entity shares what all legal entities have, without that being restated per
Schema.

A malformed built-in Schema fails startup loudly. That is a build defect, not a
runtime condition, and the difference should be visible immediately.

The catalogue shipped here is the provisional starter set named in the spec, and
should be treated as such: the open schema-catalogue ticket will replace it
wholesale, which costs almost nothing precisely because it is data.

**Blocked by:** 06

**Status:** ready-for-agent

- [ ] Built-in Schemas load from embedded YAML at startup
- [ ] The provisional starter set of entity and relation Schemas from the spec is present
- [ ] Inheritance resolves so a child Schema carries its ancestors' Properties
- [ ] Abstract Schemas cannot be instantiated
- [ ] Registry validation rejects unknown Property types, duplicate Property names after inheritance, and caption templates referencing Properties that do not exist
- [ ] A malformed built-in Schema fails startup with a message identifying it
- [ ] Captions derive from a Schema's template, with a documented rule when the template's Properties are empty
- [ ] Relation Schemas declare their endpoint constraints and whether they carry an interval
- [ ] The full resolved catalogue is readable as one structure, ready for later transports
- [ ] Golden tests pin the resolved form of every built-in Schema so an accidental ontology change is visible in review
