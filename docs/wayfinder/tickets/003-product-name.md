---
id: 003
title: Product and binary name
type: grilling
status: open
blocked_by: []
assignee:
---

## Question

What is this called?

The documentation currently uses **"OSINT Investigator"** as the product name,
`osintd` as the binary and `osint-investigator` as the service and directory
name. Those are placeholders chosen so the docs could be written; nothing is
committed to them yet.

It matters more than it looks: the name appears in the systemd unit, the
install path, the crate names, the MCP server id that agents reference in their
configuration, the database default path and every document. Changing it after
the first deployment means editing live service configuration.

**Considerations**

- Short binary names are kinder to type and to systemd (`osintd` is 6 chars).
- A generic name ("OSINT Investigator") is descriptive but unmemorable and
  collides with several existing projects and blog posts.
- A distinct name is easier to search for, and matters if the repository ever
  goes public (see [001](001-licence.md)).
- If a public repository is likely, check availability: GitHub org/repo,
  crates.io crate name, and a domain if documentation ever needs hosting.

**Decide**: keep the placeholder, or pick a name now, before M0 creates the
crates.

## Answer

_(unresolved)_
