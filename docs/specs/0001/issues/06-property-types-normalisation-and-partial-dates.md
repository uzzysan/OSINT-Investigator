# 06: Property types, normalisation and partial dates

**What to build:** the layer that lets the same fact, recorded two different ways
by two different people in two different Cases, be recognised as the same fact.

An analyst types `0601 234 567` into a Case whose region is Poland; the system
stores `+48601234567` alongside it and never discards what was typed. Another
types `12/04/1978`; it becomes a partial date at day precision. A third types
"sometime in 2019"; it becomes 2019 at year precision, and nothing anywhere
widens it into 1 January.

A checksum that fails — a transposed digit in an IBAN or a tax number — produces
a **warning**, not a rejection. This will feel wrong to anyone with a validation
habit. It is correct here: an investigator copying an identifier from a scanned
filing needs to record what the filing says, including when the filing is wrong.
Refusing the value pushes it somewhere the system cannot see.

Blocking-key derivation is defined per Property type in this ticket even though
nothing consumes it yet, because this is where that knowledge lives. The resolver
in a later milestone should find it waiting rather than invent it.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Every Property type from the data model document parses, validates and normalises
- [ ] Phone numbers normalise to E.164 using the Case's region; already-international input is unaffected
- [ ] Emails fold consistently, including tag stripping and provider-specific local-part rules
- [ ] Identifier checksums are validated for IBAN, VIN, NIP, PESEL and REGON
- [ ] A failed checksum returns a warning and the value is still usable
- [ ] URLs normalise with scheme and host folded and tracking parameters stripped
- [ ] Names fold for comparison without the stored value losing diacritics
- [ ] Partial dates carry a lower bound, an upper bound and an explicit precision
- [ ] No code path widens a partial date into a point in time
- [ ] Blocking-key derivation is implemented per Property type and unit-tested
- [ ] Normalisation is idempotent and never panics on arbitrary Unicode, proven by property-based tests
- [ ] Table-driven tests cover the awkward cases: invalid dates, two-digit years, transliterations, corrupted identifiers
