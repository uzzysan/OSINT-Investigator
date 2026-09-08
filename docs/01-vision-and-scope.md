# 1. Vision and Scope

## 1.1 The problem

Open-source investigation produces a lot of small, scattered facts: a company
number here, a phone number in a leaked listing there, a photo of a car outside
a building, a domain registration from four years ago. The facts are individually
cheap and collectively valuable — but only if they are connected.

Existing tools each solve part of this and none of it comfortably:

| Tool | Strength | Why it isn't enough here |
| --- | --- | --- |
| **Maltego** | Excellent canvas, huge transform ecosystem | Commercial, desktop-bound, no useful agent API, cases don't cross-reference |
| **OCCRP Aleph** | Superb entity ontology (FollowTheMoney), document-centric | Heavy to self-host (Elasticsearch + Postgres + Redis + workers), document-search first and graph second |
| **SpiderFoot / recon-ng** | Automated collection | Collectors, not a knowledge base; weak manual curation and no shared model |
| **Neo4j + custom UI** | Real graph engine | You build everything else yourself |
| **Obsidian / notes** | Frictionless capture | No types, no queries, no dedupe, no graph analytics |

The gap is a **curated, typed, self-hosted knowledge graph for investigations
that an AI agent can drive as competently as a human can**.

## 1.2 What we are building

A single-binary web application where:

1. An analyst creates a **Case** and builds up a graph of typed **entities** and
   **relations**, each backed by **sources**.
2. The system continuously looks for entities that might be the **same
   real-world thing** — inside the case and across every other case — and surfaces
   them as scored, explainable suggestions.
3. Confirming a cross-case match creates a **bridge**, so an investigator can
   pivot from one case into another and see the wider picture without merging
   two investigations into a mess.
4. Everything with a date lands on a **timeline** that can be scrubbed, so the
   sequence of events becomes visible rather than inferred.
5. An **MCP server** exposes the same capabilities to AI agents, which is the
   primary way bulk data gets in and reports come out.

## 1.3 Who it is for

- **Primary**: a single analyst or a very small team running investigations on a
  dedicated machine they control. Journalism, due diligence, fraud
  investigation, threat intel, corporate security, academic research.
- **Secondary**: AI agents operating on that analyst's behalf, reading and
  writing through MCP.

The first release targets **solo mode**: no login, bound to localhost or a
trusted network. See [ADR-0004](adr/0004-solo-mode-access.md) for why, and for
the seam that keeps multi-user viable later.

## 1.4 Design principles

1. **One binary, one file.** If deployment needs a wiki page, we got it wrong.
   No database server, no container runtime, no broker. Backup is `cp`.
2. **The machine proposes, the human disposes.** Automatic merging of entities
   destroys investigations. Every match is a suggestion with a visible reason.
3. **Every fact has a provenance.** A claim without a source is a hypothesis and
   is labelled as one.
4. **Agents are first-class, not bolted on.** The MCP surface is designed
   alongside the HTTP API, not derived from it afterwards.
5. **Typed, but not rigid.** Built-in schemas cover the common cases; custom
   types and properties are a normal operation, not a fork.
6. **Nothing leaves the box.** No telemetry, no outbound calls the operator did
   not configure. Investigation data is frequently sensitive.
7. **Reversible.** Merges, deletes and imports are undoable. An audit log records
   who or what changed each thing.

## 1.5 Explicitly in scope for v1

- Cases, entities, relations, properties, custom schemas
- Sources, attachments (photo/scan/PDF/screenshot), content hashing
- Graph canvas with expand, filter, path finding, saved views
- Table view and bulk edit
- Timeline view with date-precision handling
- Cross-case entity resolution with a review queue
- Full-text and fuzzy search
- CSV/XLSX import with column mapping; JSON and FollowTheMoney export
- Markdown / JSON case reports
- MCP server over stdio and streamable HTTP
- `install.sh` one-command deployment with systemd, free-port discovery, backups

## 1.6 Explicitly out of scope for v1

| Not doing | Why | Revisit when |
| --- | --- | --- |
| Built-in OSINT collectors / transforms (WHOIS, DNS, breach lookups, registries) | Agents already do this well; collectors bring API keys, rate limits and ToS liability | v2, as a plugin runner — [ADR-0003](adr/0003-agent-fed-collection.md) |
| Multi-user accounts, RBAC, SSO | Solo mode is the target; auth seam is preserved | When a second person needs their own view |
| Multi-tenancy / hosted SaaS | Different product | — |
| Document OCR and full-document indexing (Aleph's core) | Large scope; attachments are evidence, not a corpus | v2 |
| Geospatial map view | Nice, not load-bearing | v2 |
| Real-time multi-analyst collaboration (CRDT/OT) | Only meaningful with multi-user | — |
| Mobile app | The web UI is responsive; a native app adds nothing | — |
| Graph ML / link prediction | Needs data volume we don't have yet | v3 |

## 1.7 Success criteria for v1

The release is done when all of the following hold on a clean Debian/Ubuntu box:

- `git clone && ./deploy/install.sh` yields a running service in under 10 minutes
  with no manual configuration, and prints a working URL and MCP endpoint.
- An analyst can build a 200-entity case entirely through the UI without reading
  documentation.
- An AI agent, given only the MCP tool list, can create a case, add 50 entities
  with sources and relations, and produce a coherent Markdown report.
- Adding an entity that duplicates one in another case surfaces a suggestion
  within 5 seconds, with a human-readable explanation of *why*.
- A case of 20,000 entities and 60,000 relations renders and pans smoothly, and
  search returns in under 200 ms.
- Killing the process mid-write and restarting loses nothing.
