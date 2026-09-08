# OSINT Investigator

A self-hosted link-analysis workbench for open-source investigations: map people,
companies, assets, accounts and online resources as a graph, keep each
investigation in its own **Case**, and let the system tell you when a node in one
case looks like a node you already have somewhere else.

Think Maltego's canvas and OCCRP Aleph's entity model, in a single Rust binary
you can put on a box with one command — plus a first-class **MCP server** so AI
agents can read the graph, write to it, and generate reports without a browser.

> Status: **design phase**. No code yet. The full plan lives in
> [`PLAN.md`](PLAN.md); every design document is under [`docs/`](docs/).

---

## What it does

- **Cases** — investigations are isolated workspaces. A case owns its entities,
  relations, sources and timeline.
- **Typed entities** — Person, Company, Address, BankAccount, Vehicle,
  RealEstate, Website, Phone, Email, Document, Event and more, modelled after
  OCCRP's [FollowTheMoney](https://followthemoney.tech) ontology, extensible with
  your own types and properties.
- **Typed relations** — ownership, directorship, family, employment, payment,
  registration, colocation. Relations carry their own properties and validity
  intervals.
- **Cross-case matching** — a continuously running resolver proposes "this node
  might be the same as that one" across every case, with explainable scores. It
  never merges anything on its own.
- **Evidence** — every fact can carry sources: links, files, photos, scans,
  screenshots, with capture timestamps and content hashes.
- **Timeline** — anything with a date lands on a timeline; scrub through it and
  watch the graph change.
- **Reports** — export a case, a subgraph or a path as Markdown, JSON or FtM.
- **MCP server** — agents get the same capabilities the UI has, over stdio or
  streamable HTTP.

## How you deploy it

```bash
git clone <repo-url> osint-investigator
cd osint-investigator
./deploy/install.sh
```

The script provisions the toolchain if needed, builds the binary and the web
assets, picks a free port, writes a `.env`, runs the migrations, installs a
systemd service and prints the URL and the MCP endpoint. There is no database
server, no container runtime, and no message broker to operate. Backups are a
file copy.

See [`docs/08-deployment.md`](docs/08-deployment.md).

## Documentation

| Document | What's in it |
| --- | --- |
| [`docs/00-glossary.md`](docs/00-glossary.md) | Vocabulary used throughout |
| [`docs/01-vision-and-scope.md`](docs/01-vision-and-scope.md) | Who it's for, what it is and isn't |
| [`docs/02-functional-spec.md`](docs/02-functional-spec.md) | Detailed feature specification |
| [`docs/03-architecture.md`](docs/03-architecture.md) | Components, crates, request flow |
| [`docs/04-data-model.md`](docs/04-data-model.md) | Ontology and SQLite schema |
| [`docs/05-entity-resolution.md`](docs/05-entity-resolution.md) | Duplicate detection algorithm |
| [`docs/06-http-api.md`](docs/06-http-api.md) | REST surface |
| [`docs/07-mcp-server.md`](docs/07-mcp-server.md) | MCP tools, resources, prompts |
| [`docs/08-deployment.md`](docs/08-deployment.md) | install.sh, systemd, ports, backup |
| [`docs/09-security-and-legal.md`](docs/09-security-and-legal.md) | Threat model, data hygiene, GDPR notes |
| [`docs/adr/`](docs/adr/) | Architecture decision records |
| [`docs/specs/`](docs/specs/) | Agent-ready implementation specs |
| [`PLAN.md`](PLAN.md) | Living development plan — done vs. remaining |
| [`docs/wayfinder/map.md`](docs/wayfinder/map.md) | Open decisions still being charted |
| [`STATUS.md`](STATUS.md) | Session log — where work stopped, what is next |
| [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) | Working agreements and commands |

## Licence

To be decided — see [`docs/wayfinder/tickets/`](docs/wayfinder/tickets/).
