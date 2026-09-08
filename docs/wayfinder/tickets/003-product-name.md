---
id: 003
title: Product and binary name
type: grilling
status: resolved
blocked_by: []
assignee: rafal
---

## Question

What is this called?

The documentation currently uses **"Vesica"** as the product name,
`vesica` as the binary and `vesica` as the service and directory
name. Those are placeholders chosen so the docs could be written; nothing is
committed to them yet.

It matters more than it looks: the name appears in the systemd unit, the
install path, the crate names, the MCP server id that agents reference in their
configuration, the database default path and every document. Changing it after
the first deployment means editing live service configuration.

**Considerations**

- Short binary names are kinder to type and to systemd (`vesica` is 6 chars).
- A generic name ("Vesica") is descriptive but unmemorable and
  collides with several existing projects and blog posts.
- A distinct name is easier to search for, and matters if the repository ever
  goes public (see [001](001-licence.md)).
- If a public repository is likely, check availability: GitHub org/repo,
  crates.io crate name, and a domain if documentation ever needs hosting.

**Decide**: keep the placeholder, or pick a name now, before M0 creates the
crates.

## Answer

**Vesica.**

A vesica is the lens formed where two circles overlap — which is exactly what the
logomark already is, and exactly what the product does: two cases overlapping on
a shared entity. The name describes the idea rather than the category, is short
to type, and does not collide with a generic search term.

Applied throughout: crates `vesica-core`, `vesica-store`, `vesica-resolve`,
`vesica-api`, `vesica-mcp`, `vesica-report`, `vesica-import`; binary `vesica`;
service `vesica.service`; install path `/opt/vesica`; data `/var/lib/vesica`;
database `vesica.sqlite`; environment prefix `VESICA_`; MCP server id `vesica`.

The repository directory keeps its existing name; only the product does not.
Availability on GitHub, crates.io and as a domain is unchecked and worth
confirming before anything goes public — see [001](001-licence.md).
